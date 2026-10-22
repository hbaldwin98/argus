use super::*;
use crate::code::vim::motion::Find;

fn read(notation: &str, visual: bool) -> Parse {
    parse(&crate::code::vim::key::parse(notation), visual)
}

fn done(notation: &str) -> Command {
    match read(notation, false) {
        Parse::Done(c) => c,
        other => panic!("{notation:?} is {other:?}"),
    }
}

#[test]
fn a_register_and_a_count_come_before_the_command() {
    let c = done("\"a3dd");
    assert_eq!((c.register, c.count), (Some('a'), Some(3)));
    assert_eq!(c.action, Action::Operate { op: Operator::Delete, target: Target::Line });
    assert_eq!(done("3\"ayy").register, Some('a'));
}

#[test]
fn a_count_before_and_after_an_operator_multiply() {
    assert_eq!(done("2d3w").count, Some(6));
    assert_eq!(done("d3w").count, Some(3));
    assert_eq!(done("2dw").count, Some(2));
    assert_eq!(done("dw").count, None);
}

#[test]
fn zero_is_a_motion_until_a_count_has_begun() {
    assert_eq!(done("0").action, Action::Move(Motion::LineStart));
    assert_eq!(done("10j").count, Some(10));
    assert_eq!(done("d0").action, Action::Operate { op: Operator::Delete, target: Target::Motion(Motion::LineStart) });
}

#[test]
fn a_count_is_capped() {
    assert_eq!(done("99999999999j").count, Some(MOST));
}

#[test]
fn operators_take_motions_objects_and_themselves() {
    let op = |op, target| Action::Operate { op, target };
    assert_eq!(done("dw").action, op(Operator::Delete, Target::Motion(Motion::WordForward { big: false })));
    assert_eq!(done("ciw").action, op(Operator::Change, Target::Object { object: Object::Word { big: false }, inner: true }));
    assert_eq!(done("da\"").action, op(Operator::Delete, Target::Object { object: Object::Quote('"'), inner: false }));
    assert_eq!(done("yy").action, op(Operator::Yank, Target::Line));
    assert_eq!(done(">>").action, op(Operator::Indent, Target::Line));
    assert_eq!(done("<<").action, op(Operator::Outdent, Target::Line));
    assert_eq!(done("g~~").action, op(Operator::Swap, Target::Line));
    assert_eq!(done("g~g~").action, op(Operator::Swap, Target::Line));
    assert_eq!(done("gUU").action, op(Operator::Upper, Target::Line));
    assert_eq!(done("guu").action, op(Operator::Lower, Target::Line));
    assert_eq!(done("dgg").action, op(Operator::Delete, Target::Motion(Motion::FileStart)));
    assert_eq!(done("dfx").action, op(Operator::Delete, Target::Motion(Motion::Find(Find::To, 'x'))));
}

#[test]
fn the_shortcuts_are_operators() {
    let op = |op, motion| Action::Operate { op, target: Target::Motion(motion) };
    assert_eq!(done("x").action, op(Operator::Delete, Motion::Right));
    assert_eq!(done("X").action, op(Operator::Delete, Motion::Left));
    assert_eq!(done("D").action, op(Operator::Delete, Motion::LineEnd));
    assert_eq!(done("C").action, op(Operator::Change, Motion::LineEnd));
    assert_eq!(done("s").action, op(Operator::Change, Motion::Right));
    assert_eq!(done("S").action, Action::Operate { op: Operator::Change, target: Target::Line });
    assert_eq!(done("Y").action, Action::Operate { op: Operator::Yank, target: Target::Line });
}

#[test]
fn commands_of_their_own() {
    assert_eq!(done("rx").action, Action::ReplaceChar('x'));
    assert_eq!(done("J").action, Action::Join { spaces: true });
    assert_eq!(done("gJ").action, Action::Join { spaces: false });
    assert_eq!(done("gi").action, Action::Insert(At::LastInsert));
    assert_eq!(done("gI").action, Action::Insert(At::Column0));
    assert_eq!(done("gv").action, Action::Visual(Visual::Reselect));
    assert_eq!(done("]c").action, Action::NextChange);
    assert_eq!(done("[c").action, Action::PreviousChange);
    assert_eq!(done(".").action, Action::Repeat);
    assert_eq!(done("<C-r>").action, Action::Redo);
    assert_eq!(done("u").action, Action::Undo);
    assert_eq!(done(":").action, Action::CommandLine);
}

#[test]
fn half_a_command_wants_more_and_nonsense_is_bad() {
    for half in ["d", "2d", "\"", "\"a", "g", "dg", "dfx".trim_end_matches('x'), "r", "]", "di", "g~", "d2"] {
        assert_eq!(read(half, false), Parse::Need, "{half:?}");
    }
    for bad in ["dZ", "dq", "\"!", "Z", "di!", "rx1", "<C-g>", "dd2"] {
        assert_eq!(read(bad, false), Parse::Bad, "{bad:?}");
    }
}

#[test]
fn a_selection_takes_operators_at_once() {
    let on = |op, lines| Action::OnSelection { op, lines };
    let v = |n: &str| match read(n, true) {
        Parse::Done(c) => c.action,
        other => panic!("{n:?} is {other:?}"),
    };
    assert_eq!(v("d"), on(Operator::Delete, false));
    assert_eq!(v("x"), on(Operator::Delete, false));
    assert_eq!(v("D"), on(Operator::Delete, true));
    assert_eq!(v("c"), on(Operator::Change, false));
    assert_eq!(v("C"), on(Operator::Change, true));
    assert_eq!(v("S"), Action::ChangeLines);
    assert_eq!(v("R"), Action::ChangeLines);
    assert_eq!(v("y"), on(Operator::Yank, false));
    assert_eq!(v("Y"), on(Operator::Yank, true));
    assert_eq!(v(">"), on(Operator::Indent, false));
    assert_eq!(v("u"), on(Operator::Lower, false));
    assert_eq!(v("U"), on(Operator::Upper, false));
    assert_eq!(v("~"), on(Operator::Swap, false));
    assert_eq!(v("g~"), on(Operator::Swap, false));
    assert_eq!(v("o"), Action::OtherEnd { same_line: false });
    assert_eq!(v("iw"), Action::Select { object: Object::Word { big: false }, inner: true });
    assert_eq!(v("w"), Action::Move(Motion::WordForward { big: false }));
}
