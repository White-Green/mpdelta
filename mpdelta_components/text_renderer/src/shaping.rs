// 文字の中央揃えなどに使える機能を作るだけ作っておいてフロントではまだ使わない予定なので、unusedのwarningが出ている
// 面倒なので消す
#![allow(unused)]

use icu_properties::props::LineBreak;
use icu_properties::CodePointMapData;
use icu_segmenter::options::LineBreakOptions;
use std::cmp::Ordering;
use std::iter::{Enumerate, Peekable};
use std::ops::{RangeFrom, RangeTo};
use std::{mem, slice};
use swash::shape::{ShapeContext, Shaper};
use swash::text::cluster::{Boundary, CharCluster, CharInfo, Parser, Status, Token};
use swash::text::Script;
use swash::{FontRef, Setting};

#[derive(Debug, Clone)]
struct ShapingSettings<T> {
    font_size: f32,
    font: Vec<usize>,
    features: Vec<Setting<u16>>,
    variations: Vec<Setting<f32>>,
    user_data: T,
}

#[derive(Debug)]
enum ShapingSettingsEdit<T> {
    FontSize(f32),
    Font(Vec<usize>),
    PushFeature,
    PushVariation,
    UserData(T),
}

pub struct ShapingBuilder<T> {
    string_buffer: String,
    settings: Vec<(RangeFrom<usize>, ShapingSettings<T>)>,
    current_settings: ShapingSettings<T>,
}

pub struct ShapingBuilderSegment<'a, T: Clone> {
    string_buffer: &'a mut String,
    settings: &'a mut Vec<(RangeFrom<usize>, ShapingSettings<T>)>,
    current_settings: &'a mut ShapingSettings<T>,
    current_edit: ShapingSettingsEdit<T>,
}

impl<T> ShapingBuilder<T>
where
    T: Clone,
{
    pub fn new(user_data: T) -> Self {
        let current_settings = ShapingSettings {
            font_size: 0.,
            font: vec![0],
            features: Vec::new(),
            variations: Vec::new(),
            user_data,
        };
        Self {
            string_buffer: String::new(),
            settings: vec![(0.., current_settings.clone())],
            current_settings,
        }
    }

    pub fn push_str(&mut self, s: &str) -> &mut Self {
        self.string_buffer.push_str(s);
        self
    }

    pub fn font_size(&mut self, font_size: f32) -> ShapingBuilderSegment<T> {
        let ShapingBuilder { string_buffer, settings, current_settings } = self;
        let current_edit = ShapingSettingsEdit::FontSize(mem::replace(&mut current_settings.font_size, font_size));
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, current_edit)
    }

    pub fn font(&mut self, font: Vec<usize>) -> ShapingBuilderSegment<T> {
        let ShapingBuilder { string_buffer, settings, current_settings } = self;
        let current_edit = ShapingSettingsEdit::Font(mem::replace(&mut current_settings.font, font));
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, current_edit)
    }

    pub fn feature(&mut self, feature: impl Into<Setting<u16>>) -> ShapingBuilderSegment<T> {
        let ShapingBuilder { string_buffer, settings, current_settings } = self;
        current_settings.features.push(feature.into());
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, ShapingSettingsEdit::PushFeature)
    }

    pub fn variation(&mut self, variation: impl Into<Setting<f32>>) -> ShapingBuilderSegment<T> {
        let ShapingBuilder { string_buffer, settings, current_settings } = self;
        current_settings.variations.push(variation.into());
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, ShapingSettingsEdit::PushVariation)
    }

    pub fn update_user_data(&mut self, user_data_update: impl FnOnce(&T) -> T) -> ShapingBuilderSegment<T> {
        let ShapingBuilder { string_buffer, settings, current_settings } = self;
        let user_data = user_data_update(&current_settings.user_data);
        let current_edit = ShapingSettingsEdit::UserData(mem::replace(&mut current_settings.user_data, user_data));
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, current_edit)
    }

    pub fn shape(self, fonts: &[FontRef], max_width: f32) -> ShapeResult<T> {
        let ShapingBuilder { string_buffer, settings, .. } = self;
        let mut settings = settings.into_iter().peekable();
        let mut shape_context = ShapeContext::new();
        let mut cluster = CharCluster::new();
        let segmenter = icu_segmenter::LineSegmenter::new_auto(LineBreakOptions::default());
        struct ForceClone<T>(T);
        impl<T> Clone for ForceClone<T> {
            fn clone(&self) -> Self {
                unimplemented!()
            }
        }
        impl<T> Iterator for ForceClone<T>
        where
            T: Iterator,
        {
            type Item = T::Item;
            fn next(&mut self) -> Option<Self::Item> {
                self.0.next()
            }
        }
        let mut segment = segmenter.segment_str(&string_buffer).skip_while(|&i| i == 0).peekable();
        let mut parser = Parser::new(
            Script::Latin,
            ForceClone(string_buffer.char_indices().map(|(i, ch)| {
                let break_state = if segment.next_if_eq(&(i + ch.len_utf8())).is_some() {
                    let mandatory_break = matches!(CodePointMapData::<LineBreak>::new().get(ch), LineBreak::MandatoryBreak | LineBreak::CarriageReturn | LineBreak::LineFeed | LineBreak::NextLine);
                    if mandatory_break {
                        Boundary::Mandatory
                    } else {
                        Boundary::Line
                    }
                } else {
                    Boundary::None
                };
                Token {
                    ch,
                    offset: i as u32,
                    len: ch.len_utf8() as u8,
                    info: CharInfo::new(ch.into(), break_state),
                    data: 0,
                }
            })),
        );
        let (range, mut setting) = settings.next().unwrap();
        assert_eq!(range, 0..);
        let mut shaper = None::<Shaper>;
        let mut prev_font_index = usize::MAX;
        let mut lines = Vec::new();
        let mut glyphs = Vec::new();
        let mut line_y_offset = 0.;
        let mut segment_glyphs = Vec::new();
        let mut segment_advance_offset = 0.;
        let mut segment_ascent_max = 0.;
        let mut segment_descent_max = 0.;
        let mut segment_leading_max = 0.;
        let mut line_glyphs = Vec::new();
        let mut line_advance_offset = 0.;
        let mut line_ascent_max = 0.;
        let mut line_descent_max = 0.;
        let mut line_leading_max = 0.;
        let mut shape_func = |shaper: Shaper, font_id: usize, font_size: f32| {
            let metrics = shaper.metrics();
            shaper.shape_with(|cluster| {
                for glyph in cluster.glyphs {
                    (segment_ascent_max, segment_descent_max, segment_leading_max) = (metrics.ascent.max(segment_ascent_max), metrics.descent.max(segment_descent_max), metrics.leading.max(segment_leading_max));
                    segment_glyphs.push(GlyphData {
                        x: segment_advance_offset + glyph.x,
                        y: glyph.y,
                        font_id,
                        font_size,
                        glyph_id: glyph.id,
                    });
                    segment_advance_offset += glyph.advance;
                }
                if cluster.info.boundary() != Boundary::None {
                    if line_advance_offset + segment_advance_offset > max_width && !line_glyphs.is_empty() {
                        let offset_y = line_y_offset + line_ascent_max;
                        glyphs.extend(line_glyphs.drain(..).map(|data: GlyphData| data.add_y(offset_y)));
                        lines.push((..glyphs.len(), line_advance_offset));
                        line_y_offset += line_ascent_max + line_descent_max + line_leading_max;
                        line_advance_offset = 0.;
                        (line_ascent_max, line_descent_max, line_leading_max) = (0., 0., 0.);
                    }
                    line_glyphs.extend(segment_glyphs.drain(..).map(|data| data.add_x(line_advance_offset)));
                    line_advance_offset += segment_advance_offset;
                    (line_ascent_max, line_descent_max, line_leading_max) = (segment_ascent_max.max(line_ascent_max), segment_descent_max.max(line_descent_max), segment_leading_max.max(line_leading_max));
                    (segment_ascent_max, segment_descent_max, segment_leading_max) = (0., 0., 0.);
                    if line_advance_offset + segment_advance_offset > max_width || cluster.info.boundary() == Boundary::Mandatory {
                        let offset_y = line_y_offset + line_ascent_max;
                        glyphs.extend(line_glyphs.drain(..).map(|data| data.add_y(offset_y)));
                        lines.push((..glyphs.len(), line_advance_offset));
                        line_y_offset += line_ascent_max + line_descent_max + line_leading_max;
                        line_advance_offset = 0.;
                        (line_ascent_max, line_descent_max, line_leading_max) = (0., 0., 0.);
                    }
                    segment_advance_offset = 0.;
                }
            });
            glyphs.len() + line_glyphs.len() + segment_glyphs.len()
        };
        let mut proceeded = 0;
        let mut user_data = Vec::new();
        while parser.next(&mut cluster) {
            if let Some((_, new_setting)) = settings.next_if(|(range, _)| range.contains(&(cluster.range().start as usize))) {
                if let Some(shaper) = shaper.take() {
                    proceeded = shape_func(shaper, prev_font_index, setting.font_size);
                }
                user_data.push((..proceeded, setting.user_data));
                setting = new_setting;
                prev_font_index = usize::MAX;
            }
            let font_index = 'outer: {
                let mut best = None;
                for font_id in setting.font.iter().copied() {
                    match cluster.map(|ch| fonts[font_id].charmap().map(ch)) {
                        Status::Complete => break 'outer Some(font_id),
                        Status::Keep => best = Some(font_id),
                        Status::Discard => {}
                    };
                }
                best
            };
            if prev_font_index != font_index.unwrap_or(0) {
                if let Some(shaper) = shaper.take() {
                    proceeded = shape_func(shaper, prev_font_index, setting.font_size);
                }
                prev_font_index = font_index.unwrap_or(0);
            }
            // shaper.get_or_insert_with() をするとライフタイムのエラーになる なぜ
            if let Some(shaper) = shaper.as_mut() {
                shaper.add_cluster(&cluster);
            } else {
                let mut s = shape_context.builder(fonts[prev_font_index]).size(setting.font_size).features(setting.features.iter().copied()).variations(setting.variations.iter().copied()).build();
                s.add_cluster(&cluster);
                shaper = Some(s);
            }
        }
        if let Some(shaper) = shaper.take() {
            proceeded = shape_func(shaper, prev_font_index, setting.font_size);
        }
        user_data.push((..proceeded, setting.user_data));

        if line_advance_offset + segment_advance_offset > max_width && !line_glyphs.is_empty() {
            let offset_y = line_y_offset + line_ascent_max;
            glyphs.extend(line_glyphs.drain(..).map(|data| data.add_y(offset_y)));
            lines.push((..glyphs.len(), line_advance_offset));
            line_y_offset += line_ascent_max + line_descent_max + line_leading_max;
            line_advance_offset = 0.;
            (line_ascent_max, line_descent_max) = (0., 0.);
        }
        if !segment_glyphs.is_empty() {
            line_glyphs.extend(segment_glyphs.drain(..).map(|data| data.add_x(line_advance_offset)));
            line_advance_offset += segment_advance_offset;
            (line_ascent_max, line_descent_max) = (segment_ascent_max.max(line_ascent_max), segment_descent_max.max(line_descent_max));
        }
        if !line_glyphs.is_empty() {
            let offset_y = line_y_offset + line_ascent_max;
            glyphs.extend(line_glyphs.drain(..).map(|data| data.add_y(offset_y)));
            lines.push((..glyphs.len(), line_advance_offset));
            line_y_offset += line_ascent_max + line_descent_max;
        }
        ShapeResult {
            width: max_width,
            height: line_y_offset,
            glyphs,
            lines,
            user_data,
        }
    }
}

impl<'a, T> ShapingBuilderSegment<'a, T>
where
    T: Clone,
{
    fn new(string_buffer: &'a mut String, settings: &'a mut Vec<(RangeFrom<usize>, ShapingSettings<T>)>, current_settings: &'a mut ShapingSettings<T>, current_edit: ShapingSettingsEdit<T>) -> ShapingBuilderSegment<'a, T> {
        if settings.last().unwrap().0 == (string_buffer.len()..) {
            settings.last_mut().unwrap().1 = current_settings.clone();
        } else {
            settings.push((string_buffer.len().., current_settings.clone()));
        }
        ShapingBuilderSegment {
            string_buffer,
            settings,
            current_settings,
            current_edit,
        }
    }

    pub fn push_str(&mut self, s: &str) -> &mut Self {
        self.string_buffer.push_str(s);
        self
    }

    pub fn font_size(&mut self, font_size: f32) -> ShapingBuilderSegment<'_, T> {
        let ShapingBuilderSegment { string_buffer, settings, current_settings, .. } = self;
        let current_edit = ShapingSettingsEdit::FontSize(mem::replace(&mut current_settings.font_size, font_size));
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, current_edit)
    }

    pub fn font(&mut self, font: Vec<usize>) -> ShapingBuilderSegment<'_, T> {
        let ShapingBuilderSegment { string_buffer, settings, current_settings, .. } = self;
        let current_edit = ShapingSettingsEdit::Font(mem::replace(&mut current_settings.font, font));
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, current_edit)
    }

    pub fn feature(&mut self, feature: impl Into<Setting<u16>>) -> ShapingBuilderSegment<'_, T> {
        let ShapingBuilderSegment { string_buffer, settings, current_settings, .. } = self;
        current_settings.features.push(feature.into());
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, ShapingSettingsEdit::PushFeature)
    }

    pub fn variation(&mut self, variation: impl Into<Setting<f32>>) -> ShapingBuilderSegment<'_, T> {
        let ShapingBuilderSegment { string_buffer, settings, current_settings, .. } = self;
        current_settings.variations.push(variation.into());
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, ShapingSettingsEdit::PushVariation)
    }

    pub fn update_user_data(&mut self, user_data_update: impl FnOnce(&T) -> T) -> ShapingBuilderSegment<'_, T> {
        let ShapingBuilderSegment { string_buffer, settings, current_settings, .. } = self;
        let user_data = user_data_update(&current_settings.user_data);
        let current_edit = ShapingSettingsEdit::UserData(mem::replace(&mut current_settings.user_data, user_data));
        ShapingBuilderSegment::new(string_buffer, settings, current_settings, current_edit)
    }
}

impl<T> Drop for ShapingBuilderSegment<'_, T>
where
    T: Clone,
{
    fn drop(&mut self) {
        match mem::replace(&mut self.current_edit, ShapingSettingsEdit::PushFeature) {
            ShapingSettingsEdit::FontSize(font_size) => self.current_settings.font_size = font_size,
            ShapingSettingsEdit::Font(font) => self.current_settings.font = font,
            ShapingSettingsEdit::PushFeature => {
                self.current_settings.features.pop();
            }
            ShapingSettingsEdit::PushVariation => {
                self.current_settings.variations.pop();
            }
            ShapingSettingsEdit::UserData(user_data) => self.current_settings.user_data = user_data,
        }
        if self.settings.last().unwrap().0 == (self.string_buffer.len()..) {
            self.settings.last_mut().unwrap().1 = self.current_settings.clone();
        } else {
            self.settings.push((self.string_buffer.len().., self.current_settings.clone()));
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphData {
    pub x: f32,
    pub y: f32,
    pub font_id: usize,
    pub font_size: f32,
    pub glyph_id: u16,
}

impl GlyphData {
    fn add_x(self, offset_x: f32) -> Self {
        Self { x: self.x + offset_x, ..self }
    }

    fn add_y(self, offset_y: f32) -> Self {
        Self { y: self.y + offset_y, ..self }
    }
}

#[derive(Debug)]
pub struct ShapeResult<T> {
    width: f32,
    height: f32,
    glyphs: Vec<GlyphData>,
    lines: Vec<(RangeTo<usize>, f32)>,
    user_data: Vec<(RangeTo<usize>, T)>,
}

#[derive(Debug)]
pub struct ShapeResultGlyphsIter<'a, T> {
    glyphs: Enumerate<slice::Iter<'a, GlyphData>>,
    user_data: Peekable<slice::Iter<'a, (RangeTo<usize>, T)>>,
}

#[derive(Debug)]
pub struct ShapeResultLinesIter<'a, T> {
    glyph_offset: usize,
    user_data_offset: usize,
    glyphs: &'a [GlyphData],
    lines: slice::Iter<'a, (RangeTo<usize>, f32)>,
    user_data: &'a [(RangeTo<usize>, T)],
}

#[derive(Debug)]
pub struct ShapeResultLine<'a, T> {
    offset: usize,
    width: f32,
    glyphs: &'a [GlyphData],
    user_data: &'a [(RangeTo<usize>, T)],
}

#[derive(Debug)]
pub struct ShapeResultLineIter<'a, T> {
    offset: usize,
    glyphs: Enumerate<slice::Iter<'a, GlyphData>>,
    user_data: Peekable<slice::Iter<'a, (RangeTo<usize>, T)>>,
}

impl<T> ShapeResult<T> {
    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn glyphs(&self) -> ShapeResultGlyphsIter<T> {
        ShapeResultGlyphsIter {
            glyphs: self.glyphs.iter().enumerate(),
            user_data: self.user_data.iter().peekable(),
        }
    }

    pub fn lines(&self) -> ShapeResultLinesIter<T> {
        ShapeResultLinesIter {
            glyph_offset: 0,
            user_data_offset: 0,
            glyphs: &self.glyphs,
            lines: self.lines.iter(),
            user_data: &self.user_data,
        }
    }
}

impl<'a, T> Iterator for ShapeResultGlyphsIter<'a, T> {
    type Item = (GlyphData, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.glyphs.next().map(|(i, &glyph)| {
            while self.user_data.next_if(|(range, _)| !range.contains(&i)).is_some() {}
            let (_, user_data) = self.user_data.peek().unwrap();
            (glyph, user_data)
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.glyphs.size_hint()
    }
}

impl<T> ExactSizeIterator for ShapeResultGlyphsIter<'_, T> {}

impl<'a, T> Iterator for ShapeResultLinesIter<'a, T> {
    type Item = ShapeResultLine<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.lines.next().map(|&(ref range, width)| {
            let line_glyphs = &self.glyphs[self.glyph_offset..range.end];
            let line_user_data = &self.user_data[self.user_data_offset..];
            self.user_data_offset += line_user_data.binary_search_by(|(user_data_range, _)| if user_data_range.contains(&range.end) { Ordering::Greater } else { Ordering::Less }).unwrap_err();
            let offset = mem::replace(&mut self.glyph_offset, range.end);
            ShapeResultLine {
                offset,
                width,
                glyphs: line_glyphs,
                user_data: line_user_data,
            }
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.lines.size_hint()
    }
}

impl<T> ExactSizeIterator for ShapeResultLinesIter<'_, T> {}

impl<'a, T> ShapeResultLine<'a, T> {
    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn iter(&self) -> ShapeResultLineIter<'a, T> {
        ShapeResultLineIter {
            offset: self.offset,
            glyphs: self.glyphs.iter().enumerate(),
            user_data: self.user_data.iter().peekable(),
        }
    }
}

impl<'a, T> IntoIterator for ShapeResultLine<'a, T> {
    type Item = (GlyphData, &'a T);
    type IntoIter = ShapeResultLineIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        ShapeResultLineIter {
            offset: self.offset,
            glyphs: self.glyphs.iter().enumerate(),
            user_data: self.user_data.iter().peekable(),
        }
    }
}

impl<'a, T> Iterator for ShapeResultLineIter<'a, T> {
    type Item = (GlyphData, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        self.glyphs.next().map(|(i, &glyph)| {
            while self.user_data.next_if(|(range, _)| !range.contains(&(self.offset + i))).is_some() {}
            let (_, user_data) = self.user_data.peek().unwrap();
            (glyph, user_data)
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.glyphs.size_hint()
    }
}

impl<T> ExactSizeIterator for ShapeResultLineIter<'_, T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::Path;
    use swash::scale::ScaleContext;
    use swash::zeno::{Command, PathData, Point};
    use swash::FontRef;

    #[test]
    fn test_text_shaping() {
        const TEST_OUTPUT_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test_output/", env!("CARGO_PKG_NAME"));
        let output_file_dir = Path::new(TEST_OUTPUT_DIR).join("text_shaping");
        fs::create_dir_all(&output_file_dir).unwrap();

        let noto_sans_jp = [
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-Thin.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-Black.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-Bold.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-ExtraBold.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-ExtraLight.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-Light.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-Medium.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-Regular.ttf"), 0).unwrap(),
            FontRef::from_index(include_bytes!("fonts/Noto_Sans_JP/NotoSansJP-SemiBold.ttf"), 0).unwrap(),
        ];
        let mut scale_context = ScaleContext::new();

        let fonts = noto_sans_jp;
        let mut builder = ShapingBuilder::new(());
        builder
            .font_size(16.)
            .push_str("The ")
            .font(vec![1])
            .push_str("quick ")
            .font(vec![2])
            .push_str("brown ")
            .font(vec![3])
            .font_size(24.)
            .push_str("fox ")
            .font(vec![4])
            .push_str("jumps ")
            .font(vec![5])
            .push_str("over ")
            .font(vec![6])
            .font_size(16.)
            .push_str("the ")
            .font(vec![7])
            .push_str("lazy ")
            .font(vec![8])
            .push_str("dog.");
        let result = builder.shape(&fonts, 200.);

        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(output_file_dir.join("glyphs.svg")).unwrap();
        write!(file, "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">", result.width(), result.height()).unwrap();
        for (GlyphData { x: offset_x, y: offset_y, font_id, font_size, glyph_id }, ()) in result.glyphs() {
            if let Some(outline) = scale_context.builder(fonts[font_id]).size(font_size).build().scale_outline(glyph_id) {
                write!(file, "<path d=\"").unwrap();
                for command in outline.path().commands() {
                    match command {
                        Command::MoveTo(Point { x, y }) => write!(file, "M{} {}", x + offset_x, -y + offset_y).unwrap(),
                        Command::LineTo(Point { x, y }) => write!(file, "L{} {}", x + offset_x, -y + offset_y).unwrap(),
                        Command::CurveTo(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }, Point { x: x2, y: y2 }) => write!(file, "C{} {} {} {} {} {}", x0 + offset_x, -y0 + offset_y, x1 + offset_x, -y1 + offset_y, x2 + offset_x, -y2 + offset_y).unwrap(),
                        Command::QuadTo(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }) => write!(file, "Q{} {} {} {}", x0 + offset_x, -y0 + offset_y, x1 + offset_x, -y1 + offset_y).unwrap(),
                        Command::Close => write!(file, "Z").unwrap(),
                    }
                }
                write!(file, "\" fill=\"black\" stroke=\"none\"/>").unwrap();
            }
        }
        write!(file, "</svg>").unwrap();

        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(output_file_dir.join("lines.svg")).unwrap();
        write!(file, "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">", result.width(), result.height()).unwrap();
        for line in result.lines() {
            let x = (result.width() - line.width()) / 2.;
            for (GlyphData { x: offset_x, y: offset_y, font_id, font_size, glyph_id }, ()) in line {
                let offset_x = x + offset_x;
                if let Some(outline) = scale_context.builder(fonts[font_id]).size(font_size).build().scale_outline(glyph_id) {
                    write!(file, "<path d=\"").unwrap();
                    for command in outline.path().commands() {
                        match command {
                            Command::MoveTo(Point { x, y }) => write!(file, "M{} {}", x + offset_x, -y + offset_y).unwrap(),
                            Command::LineTo(Point { x, y }) => write!(file, "L{} {}", x + offset_x, -y + offset_y).unwrap(),
                            Command::CurveTo(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }, Point { x: x2, y: y2 }) => write!(file, "C{} {} {} {} {} {}", x0 + offset_x, -y0 + offset_y, x1 + offset_x, -y1 + offset_y, x2 + offset_x, -y2 + offset_y).unwrap(),
                            Command::QuadTo(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }) => write!(file, "Q{} {} {} {}", x0 + offset_x, -y0 + offset_y, x1 + offset_x, -y1 + offset_y).unwrap(),
                            Command::Close => write!(file, "Z").unwrap(),
                        }
                    }
                    write!(file, "\" fill=\"black\" stroke=\"none\"/>").unwrap();
                }
            }
        }
        write!(file, "</svg>").unwrap();

        let fonts = [noto_sans_jp[6], noto_sans_jp[2]];
        let mut builder = ShapingBuilder::new(0usize);
        for (i, c) in "The quick brown fox jumps over the lazy dog.".chars().enumerate() {
            builder.font_size(16.).update_user_data(|_| i).push_str(c.encode_utf8(&mut [0; 4]));
        }
        let result = builder.shape(&fonts, 100.);
        result.glyphs().zip(0..).for_each(|((_, &user_data), i)| assert_eq!(user_data, i));
        result.lines().flatten().zip(0..).for_each(|((_, &user_data), i)| assert_eq!(user_data, i));

        let fonts = [noto_sans_jp[6], noto_sans_jp[2]];
        let mut builder = ShapingBuilder::new(());
        builder
            .font_size(16.)
            .font(vec![0, 1])
            .push_str("あのイーハトーヴォのすきとおったWind、Summerでも底に冷たさをもつ青いSky、うつくしいForestで飾られたモリーオ市、郊外のぎらぎらひかるGrassの波。");
        let result = builder.shape(&fonts, 200.);

        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(output_file_dir.join("japanese.svg")).unwrap();
        write!(file, "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">", result.width(), result.height()).unwrap();
        for (GlyphData { x: offset_x, y: offset_y, font_id, font_size, glyph_id }, ()) in result.glyphs() {
            let metrics = fonts[font_id].metrics(&[]);
            let scale = font_size / metrics.units_per_em as f32;
            if let Some(outline) = scale_context.builder(fonts[font_id]).build().scale_outline(glyph_id) {
                write!(file, "<path d=\"").unwrap();
                let map_x = |x: f32| x * scale + offset_x;
                let map_y = |y: f32| -y * scale + offset_y;
                for command in outline.path().commands() {
                    match command {
                        Command::MoveTo(Point { x, y }) => write!(file, "M{} {}", map_x(x), map_y(y)).unwrap(),
                        Command::LineTo(Point { x, y }) => write!(file, "L{} {}", map_x(x), map_y(y)).unwrap(),
                        Command::CurveTo(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }, Point { x: x2, y: y2 }) => write!(file, "C{} {} {} {} {} {}", map_x(x0), map_y(y0), map_x(x1), map_y(y1), map_x(x2), map_y(y2)).unwrap(),
                        Command::QuadTo(Point { x: x0, y: y0 }, Point { x: x1, y: y1 }) => write!(file, "Q{} {} {} {}", map_x(x0), map_y(y0), map_x(x1), map_y(y1)).unwrap(),
                        Command::Close => write!(file, "Z").unwrap(),
                    }
                }
                write!(file, "\" fill=\"black\" stroke=\"none\"/>").unwrap();
            }
        }
        write!(file, "</svg>").unwrap();
    }
}
