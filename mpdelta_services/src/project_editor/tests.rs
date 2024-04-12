use crate::project_editor::ProjectEditor;
use mpdelta_core::component::parameter::ParameterValueType;
use mpdelta_core::core::Editor;
use mpdelta_core::edit::RootComponentEditCommand;
use mpdelta_core::mfrac;
use mpdelta_core::time::TimelineTime;
use mpdelta_core_test_util::{assert_eq_root_component_class_ignore_cached_time, marker, root_component_class};
use qcell::TCellOwner;
use std::sync::Arc;
use tokio::sync::RwLock;

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
    struct K;
    let key = Arc::new(RwLock::new(TCellOwner::new()));
    macro_rules! key {
        () => {
            *key.read().await
        };
    }
    let editor = ProjectEditor::new(Arc::clone(&key));
    root_component_class! {
        edit_target = <K, T> key!();
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
        expect = <K, T> key!();
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!() => r1] },
        ],
        links: [
            left = mfrac!(2) => l1,
            l1 = mfrac!(1) => r1,
        ],
    }
    assert_eq_root_component_class_ignore_cached_time(&edit_target, &expect, &key!()).await;
}

#[tokio::test]
async fn test_delete_component_instance() {
    struct K;
    let key = Arc::new(RwLock::new(TCellOwner::new()));
    macro_rules! key {
        () => {
            *key.read().await
        };
    }
    let editor = ProjectEditor::new(Arc::clone(&key));
    root_component_class! {
        edit_target = <K, T> key!();
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
        expect = <K, T> key!();
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
        ],
        links: [
            left = mfrac!(2) => l1,
        ],
    }
    assert_eq_root_component_class_ignore_cached_time(&edit_target, &expect, &key!()).await;
    root_component_class! {
        edit_target = <K, T> key!();
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
        expect = <K, T> key!();
        left: left,
        components: [
            { markers: [marker!(locked: 0) => l1, marker!(locked: 1) => r1] },
        ],
        links: [
            left = mfrac!(1) => l1,
        ],
    }
    assert_eq_root_component_class_ignore_cached_time(&edit_target, &expect, &key!()).await;
    root_component_class! {
        edit_target = <K, T> key!();
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
        expect = <K, T> key!();
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
    assert_eq_root_component_class_ignore_cached_time(&edit_target, &expect, &key!()).await;
}

#[tokio::test]
async fn test_connect_marker_pins() {
    struct K;
    let key = Arc::new(RwLock::new(TCellOwner::new()));
    macro_rules! key {
        () => {
            *key.read().await
        };
    }
    let editor = ProjectEditor::new(Arc::clone(&key));
    root_component_class! {
        edit_target = <K, T> key!();
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
        expect = <K, T> key!();
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
    assert_eq_root_component_class_ignore_cached_time(&edit_target, &expect, &key!()).await;
}
