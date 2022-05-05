use crate::common::mapped_slice::MappedSliceMut;
use crate::component::parameter::{ParameterValue, ParameterValueViewForFix};

pub trait ComponentProcessor {
    fn fix_parameter(&self, params: MappedSliceMut<ParameterValue, &ParameterValue, ParameterValueViewForFix>);
}
