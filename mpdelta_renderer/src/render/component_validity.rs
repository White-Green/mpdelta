use crate::invalidate_range::InvalidateRange;
use crate::render::ComponentRenderer;
use cgmath::Vector3;
use mpdelta_core::component::instance::{ComponentInstance, ComponentInstanceId};
use mpdelta_core::component::marker_pin::MarkerPinId;
use mpdelta_core::component::parameter::{ImageRequiredParamsTransform, ParameterValueType, Vector3Params};
use mpdelta_core::time::TimelineTime;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

#[derive(Debug)]
pub enum ImageRequiredParamsInvalidateRangeTransform {
    Params {
        size: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        scale: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        translate: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        scale_center: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        rotate_center: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
    },
    Free {
        left_top: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        right_top: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        left_bottom: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
        right_bottom: Vector3<Vec<Arc<ComponentInvalidateRange>>>,
    },
}

#[derive(Debug)]
pub struct ImageRequiredParamsInvalidateRange {
    pub transform: Arc<ImageRequiredParamsInvalidateRangeTransform>,
}

#[derive(Debug)]
pub struct ComponentInvalidateRange {
    pub invalidate_range: InvalidateRange<TimelineTime>,
    pub image_required_params: Option<ImageRequiredParamsInvalidateRange>,
    pub audio_required_params: Option<Vec<Arc<[Arc<ComponentInvalidateRange>]>>>,
    pub variable_parameters: Vec<Arc<[Arc<ComponentInvalidateRange>]>>,
}

impl ComponentInvalidateRange {
    pub fn new_default<T>(component: &ComponentInstance<T>) -> ComponentInvalidateRange
    where
        T: ParameterValueType,
    {
        let image_required_params = component.image_required_params().map(|params| match &*params.transform {
            ImageRequiredParamsTransform::Params { .. } => ImageRequiredParamsInvalidateRange {
                transform: Arc::new(ImageRequiredParamsInvalidateRangeTransform::Params {
                    size: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    scale: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    translate: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    scale_center: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    rotate_center: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                }),
            },
            ImageRequiredParamsTransform::Free { .. } => ImageRequiredParamsInvalidateRange {
                transform: Arc::new(ImageRequiredParamsInvalidateRangeTransform::Free {
                    left_top: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    right_top: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    left_bottom: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                    right_bottom: Vector3::new(Vec::new(), Vec::new(), Vec::new()),
                }),
            },
        });
        let audio_required_params = component.audio_required_params().map(|params| vec![Arc::new([]) as Arc<[_]>; params.volume.len()]);
        let variable_parameters = vec![Arc::new([]) as Arc<[_]>; component.variable_parameters().len()];
        ComponentInvalidateRange {
            invalidate_range: InvalidateRange::new(),
            image_required_params,
            audio_required_params,
            variable_parameters,
        }
    }
}

pub fn collect_invalidate_range<T, I, A, C>(components: &[ComponentInstanceId], component_map: &HashMap<ComponentInstanceId, Arc<ComponentRenderer<T, I, A, C>>>, time_map: &HashMap<MarkerPinId, TimelineTime>) -> Vec<Arc<ComponentInvalidateRange>>
where
    T: ParameterValueType,
{
    struct Context<'a, T, I, A, C>
    where
        T: ParameterValueType,
    {
        invalidate_ranges: HashMap<ComponentInstanceId, InvalidateRange<TimelineTime>>,
        component_map: &'a HashMap<ComponentInstanceId, Arc<ComponentRenderer<T, I, A, C>>>,
        time_map: &'a HashMap<MarkerPinId, TimelineTime>,
    }

    fn traverse<T, I, A, C>(ctx: &mut Context<T, I, A, C>, c: &ComponentInstanceId, range: Range<TimelineTime>) -> Arc<ComponentInvalidateRange>
    where
        T: ParameterValueType,
    {
        let mut result = inner(ctx, c);
        let invalidate_range = ctx.invalidate_ranges.entry(*c).or_default();
        result.invalidate_range = invalidate_range.clone();
        invalidate_range.invalidate(range.clone());
        Arc::new(result)
    }

    fn inner<T, I, A, C>(ctx: &mut Context<T, I, A, C>, component: &ComponentInstanceId) -> ComponentInvalidateRange
    where
        T: ParameterValueType,
    {
        let component = &ctx.component_map[component].component;
        let range = ctx.time_map[component.marker_left().id()]..ctx.time_map[component.marker_right().id()];
        let image_required_params = component.image_required_params().map(|params| {
            let mut convert_vector3 = |param: &Vector3Params| AsRef::<[_; 3]>::as_ref(param).each_ref().map(|p| p.components.iter().map(|c| traverse(ctx, c, range.clone())).collect::<Vec<_>>()).into();
            let transform = match &*params.transform {
                ImageRequiredParamsTransform::Params {
                    size,
                    scale,
                    translate,
                    rotate: _,
                    scale_center,
                    rotate_center,
                } => ImageRequiredParamsInvalidateRangeTransform::Params {
                    size: convert_vector3(size),
                    scale: convert_vector3(scale),
                    translate: convert_vector3(translate),
                    scale_center: convert_vector3(scale_center),
                    rotate_center: convert_vector3(rotate_center),
                },
                ImageRequiredParamsTransform::Free { left_top, right_top, left_bottom, right_bottom } => ImageRequiredParamsInvalidateRangeTransform::Free {
                    left_top: convert_vector3(left_top),
                    right_top: convert_vector3(right_top),
                    left_bottom: convert_vector3(left_bottom),
                    right_bottom: convert_vector3(right_bottom),
                },
            };
            ImageRequiredParamsInvalidateRange { transform: Arc::new(transform) }
        });
        let audio_required_params = component.audio_required_params().map(|params| params.volume.iter().map(|volume| volume.components.iter().map(|c| traverse(ctx, c, range.clone())).collect()).collect());
        let variable_parameters = component.variable_parameters().iter().map(|param| param.components.iter().map(|c| traverse(ctx, c, range.clone())).collect()).collect();
        ComponentInvalidateRange {
            invalidate_range: InvalidateRange::new(),
            image_required_params,
            audio_required_params,
            variable_parameters,
        }
    }

    let mut context = Context {
        invalidate_ranges: HashMap::new(),
        component_map,
        time_map,
    };
    let mut result = components.iter().map(|component| Arc::new(inner(&mut context, component))).collect::<Vec<_>>();
    result.iter_mut().zip(components).for_each(|(i, id)| Arc::get_mut(i).unwrap().invalidate_range = context.invalidate_ranges.get(id).map_or_else(Default::default, Clone::clone));
    result
}
