//! `true` and `false` — exit 0 / exit 1 unconditionally.

use crate::registry::Applet;

pub const TRUE: Applet = Applet {
    name: "true",
    help: "do nothing, successfully",
    aliases: &[],
    main: main_true,
};

pub const FALSE: Applet = Applet {
    name: "false",
    help: "do nothing, unsuccessfully",
    aliases: &[],
    main: main_false,
};

fn main_true(_argv: &[String]) -> i32 {
    0
}

fn main_false(_argv: &[String]) -> i32 {
    1
}
