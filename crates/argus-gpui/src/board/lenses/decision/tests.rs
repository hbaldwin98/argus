use super::*;

// BOARD-RECORD-6: `+` is a gain and `-` a cost, and the words are what follows the sign.
#[test]
fn a_consequence_is_a_gain_or_a_cost_and_its_words_follow_the_sign() {
    assert_eq!(consequence("+ a prompt pays only for the feature it works on"), (true, "a prompt pays only for the feature it works on"));
    assert_eq!(consequence("- one more request"), (false, "one more request"));
    assert_eq!(consequence("+no space"), (true, "no space"));
    assert_eq!(consequence("--double"), (false, "double"), "a leading run of signs is the sign");
}
