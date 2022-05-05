use crate::component::class::ComponentClass;
use crate::component::marker_pin::MarkerPin;
use crate::component::parameter::{AudioRequiredParams, ImageRequiredParams, ParameterValue};
use crate::ptr::{StaticPointer, StaticPointerOwned};
use std::sync::RwLock;

type Cell<T> = RwLock<T>;

pub struct ComponentInstance {
    component_class: StaticPointer<ComponentClass>,
    marker_left: StaticPointerOwned<Cell<MarkerPin>>,
    marker_right: StaticPointerOwned<Cell<MarkerPin>>,
    markers: Vec<StaticPointerOwned<Cell<MarkerPin>>>,
    image_required_params: Option<ImageRequiredParams>,
    audio_required_params: Option<AudioRequiredParams>,
    parameters: Vec<ParameterValue<StaticPointer<MarkerPin>>>,
    processor: (), // TODO:処理系を詰めないとどういう構成にするか決まらないのでとりあえず無を置いておく
}

impl ComponentInstance {
    pub fn component_class(&self) -> &StaticPointer<ComponentClass> {
        &self.component_class
    }
    pub fn marker_left(&self) -> &StaticPointerOwned<Cell<MarkerPin>> {
        &self.marker_left
    }
    pub fn marker_right(&self) -> &StaticPointerOwned<Cell<MarkerPin>> {
        &self.marker_right
    }
    pub fn markers(&self) -> &[StaticPointerOwned<Cell<MarkerPin>>] {
        &self.markers
    }
    pub fn image_required_params(&self) -> Option<&ImageRequiredParams> {
        self.image_required_params.as_ref()
    }
    pub fn audio_required_params(&self) -> Option<&AudioRequiredParams> {
        self.audio_required_params.as_ref()
    }
    pub fn parameters(&self) -> &[ParameterValue<StaticPointer<MarkerPin>>] {
        &self.parameters
    }
}
