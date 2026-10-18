//! Requests about what agents and extensions show: where the person is,
//! agents' views and saved views and presses on them, and extensions'
//! actions, commands and dialogs. A host runs no agent view and no
//! extension, so most of these name something that is not there.

use argus_protocol::{Answer, AnswerValue, ClientMsg, ExtAnswer, View, ViewPress};
use proptest::prelude::*;

use super::values::{checkout, maybe_text, pane, project, text};
use super::Known;

fn view_of(k: &Known) -> BoxedStrategy<View> {
    (proptest::option::of(project(k)), proptest::option::of(checkout(k)), proptest::option::of(pane(k)), any::<bool>())
        .prop_map(|(project, checkout, pane, active)| View { project, checkout, pane, active })
        .boxed()
}

fn press() -> BoxedStrategy<ViewPress> {
    let value = prop_oneof![
        text().prop_map(AnswerValue::Text),
        any::<f64>().prop_map(AnswerValue::Number),
        any::<bool>().prop_map(AnswerValue::Bool),
    ];
    let answer = (text(), value).prop_map(|(name, value)| Answer { name, value });
    (text(), text(), proptest::collection::vec(text(), 0..3), maybe_text(), proptest::collection::vec(answer, 0..3))
        .prop_map(|(view, action, rows, form, answers)| ViewPress { view, action, rows, form, answers })
        .boxed()
}

pub fn view(k: &Known) -> BoxedStrategy<ClientMsg> {
    view_of(k).prop_map(ClientMsg::View).boxed()
}

pub fn close_view(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), text()).prop_map(|(pane, id)| ClientMsg::CloseView { pane, id }).boxed()
}

pub fn refresh_view(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), text()).prop_map(|(pane, id)| ClientMsg::RefreshView { pane, id }).boxed()
}

pub fn press_view(k: &Known) -> BoxedStrategy<ClientMsg> {
    (pane(k), press()).prop_map(|(pane, press)| ClientMsg::PressView { pane, press }).boxed()
}

pub fn refresh_saved_view() -> BoxedStrategy<ClientMsg> {
    text().prop_map(|id| ClientMsg::RefreshSavedView { id }).boxed()
}

pub fn mark_saved_view_stale() -> BoxedStrategy<ClientMsg> {
    text().prop_map(|id| ClientMsg::MarkSavedViewStale { id }).boxed()
}

pub fn delete_saved_view() -> BoxedStrategy<ClientMsg> {
    text().prop_map(|id| ClientMsg::DeleteSavedView { id }).boxed()
}

pub fn press_saved_view() -> BoxedStrategy<ClientMsg> {
    (text(), press()).prop_map(|(id, press)| ClientMsg::PressSavedView { id, press }).boxed()
}

pub fn extension_action() -> BoxedStrategy<ClientMsg> {
    (text(), text(), maybe_text(), text())
        .prop_map(|(extension, view, row, action)| ClientMsg::ExtensionAction { extension, view, row, action })
        .boxed()
}

pub fn extension_command(k: &Known) -> BoxedStrategy<ClientMsg> {
    (text(), text(), view_of(k))
        .prop_map(|(extension, command, context)| ClientMsg::ExtensionCommand { extension, command, context })
        .boxed()
}

pub fn extension_dialog_answer() -> BoxedStrategy<ClientMsg> {
    let form = proptest::collection::btree_map(text(), text(), 0..3);
    let answer = prop_oneof![
        Just(ExtAnswer::Yes),
        text().prop_map(ExtAnswer::Text),
        text().prop_map(ExtAnswer::Picked),
        form.prop_map(ExtAnswer::Form),
    ];
    (prop_oneof![0..4u64, any::<u64>()], proptest::option::of(answer))
        .prop_map(|(dialog, answer)| ClientMsg::ExtensionDialogAnswer { dialog, answer })
        .boxed()
}
