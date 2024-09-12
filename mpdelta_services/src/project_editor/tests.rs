use crate::project_editor::ProjectEditor;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::Editor;
use mpdelta_core::edit::{InstanceEditCommand, RootComponentEditCommand};
use mpdelta_core::mfrac;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_test_util::{assert_eq_root_component_class, root_component_class, TestIdGenerator};
use std::sync::Arc;

struct T;

impl ParameterValueType for T {
    type Image = ();
    type Audio = ();
    type Binary = ();
    type String = ();
    type Integer = ();
    type RealNumber = ();
    type Boolean = ();
    type Dictionary = ();
    type Array = ();
    type ComponentClass = ();
}

#[tokio::test]
async fn test_edit_marker_link_length() {
    let id = Arc::new(TestIdGenerator::new());
    let editor = ProjectEditor::new(Arc::clone(&id));
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!() => r1] },
        ],
        links: [
            left = mfrac!(1) => l1; link1,
            l1 = mfrac!(1) => r1,
        ],
    }
    editor.edit(edit_target.as_ref(), RootComponentEditCommand::EditMarkerLinkLength(link1.clone(), TimelineTime::new(mfrac!(2)))).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!() => r1] },
        ],
        links: [
            left = mfrac!(2) => l1,
            l1 = mfrac!(1) => r1,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
}

#[tokio::test]
async fn test_delete_component_instance() {
    let id = Arc::new(TestIdGenerator::new());
    let editor = ProjectEditor::new(Arc::clone(&id));
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] }; c1,
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] },
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => l2,
        ],
    }
    editor.edit(edit_target.as_ref(), RootComponentEditCommand::DeleteComponentInstance(c1)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
        ],
        links: [
            left = mfrac!(2) => l1,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] }; c2,
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => l2,
        ],
    }
    editor.edit(edit_target.as_ref(), RootComponentEditCommand::DeleteComponentInstance(c2)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
        ],
        links: [
            left = mfrac!(1) => l1,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] }; c2,
            { markers: [marker!(locked: 0) => l3, marker!(locked: 1) => r3] },
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => l2,
            l2 = mfrac!(1) => r2,
            r2 = mfrac!(1) => l3,
        ],
    }
    editor.edit(edit_target.as_ref(), RootComponentEditCommand::DeleteComponentInstance(c2)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l3, marker!(locked: 1) => r3] },
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(3) => l3,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
}

#[tokio::test]
async fn test_insert_component_instance_to() {
    let id = Arc::new(TestIdGenerator::new());
    let editor = ProjectEditor::new(Arc::clone(&id));
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] },
            { markers: [marker!(locked: 0) => l3, marker!(locked: 1) => r3] }; c,
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => l2,
            l2 = mfrac!(1) => r2,
            r2 = mfrac!(1) => l3,
        ],
    }
    editor.edit(edit_target.as_ref(), RootComponentEditCommand::InsertComponentInstanceTo(c, 1)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l3, marker!(locked: 1) => r3] }; c,
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] },
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => l2,
            l2 = mfrac!(1) => r2,
            r2 = mfrac!(1) => l3,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
}

#[tokio::test]
async fn test_connect_marker_pins() {
    let id = Arc::new(TestIdGenerator::new());
    let editor = ProjectEditor::new(Arc::clone(&id));
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] },
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => r1,
            l2 = mfrac!(1) => r2,
            l1 = mfrac!(1) => l2,
        ],
    }
    editor.edit(edit_target.as_ref(), RootComponentEditCommand::ConnectMarkerPins(l1, r2)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 0) => l2, marker!(locked: 1) => r2] },
        ],
        links: [
            left = mfrac!(1) => l1,
            l1 = mfrac!(1) => r1,
            l1 = mfrac!(1) => l2,
            l1 = mfrac!(2) => r2,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
}

#[tokio::test]
async fn test_lock_marker_pin() {
    let id = Arc::new(TestIdGenerator::new());
    let editor = ProjectEditor::new(Arc::clone(&id));
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!() => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
            l1 = 1 => r1,
        ],
    }
    editor.edit_instance(edit_target.as_ref(), &c1, InstanceEditCommand::LockMarkerPin(r1)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
            l1 = 1 => r1,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;

    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!() => m, marker!(locked: 2) => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
            l1 = 2 => m,
            l1 = 3 => r1,
        ],
    }
    editor.edit_instance(edit_target.as_ref(), &c1, InstanceEditCommand::LockMarkerPin(m)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: mfrac!(1, 1, 3)) => m, marker!(locked: 2) => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
            l1 = 2 => m,
            l1 = 3 => r1,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
}

#[tokio::test]
async fn test_split_at_pin() {
    let id = Arc::new(TestIdGenerator::new());
    let editor = ProjectEditor::new(Arc::clone(&id));
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!(locked: 2) => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
            l1 = 1 => m,
        ],
    }
    editor.edit_instance(edit_target.as_ref(), &c1, InstanceEditCommand::SplitAtPin(m)).await.unwrap();
    root_component_class! {
        expect; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
            { markers: [marker!(locked: 1) => l2, marker!(locked: 2) => r2] },
        ],
        links: [
            left = 1 => l1,
            l1 = 1 => r1,
            l1 = 1 => l2,
        ],
    }
    assert_eq_root_component_class(&edit_target, &expect).await;
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!() => m, marker!(locked: 2) => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
            l1 = 1 => m,
        ],
    }
    editor.edit_instance(edit_target.as_ref(), &c1, InstanceEditCommand::SplitAtPin(m)).await.unwrap_err();
    root_component_class! {
        edit_target; <T>; id;
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => m, marker!(locked: 2) => r1] }; c1,
        ],
        links: [
            left = 1 => l1,
        ],
    }
    editor.edit_instance(edit_target.as_ref(), &c1, InstanceEditCommand::SplitAtPin(m)).await.unwrap_err();
}
