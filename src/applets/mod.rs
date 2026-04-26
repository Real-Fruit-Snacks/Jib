//! All registered applets, gated by Cargo feature so a slim build can
//! compile without pulling in archive/hash/network/json modules.
//!
//! To add an applet:
//!  1. Create `src/applets/<name>.rs` exporting a `pub const APPLET: Applet`.
//!  2. Add `pub mod <name>;` below under the right feature group.
//!  3. Add `<name>::APPLET,` to the [`ALL`] slice under the same gate.
//!  4. Optionally add a `--help` body to `src/usage.rs`.

use crate::registry::Applet;

// --- Always available -------------------------------------------------------

#[cfg(feature = "slim")]
pub mod awk;
#[cfg(feature = "slim")]
pub mod basename;
#[cfg(feature = "slim")]
pub mod bools;
#[cfg(feature = "slim")]
pub mod cat;
#[cfg(feature = "slim")]
pub mod chmod;
#[cfg(feature = "slim")]
pub mod cp;
#[cfg(feature = "slim")]
pub mod cut;
#[cfg(feature = "slim")]
pub mod date;
#[cfg(feature = "slim")]
pub mod dirname;
#[cfg(feature = "slim")]
pub mod echo;
#[cfg(feature = "slim")]
pub mod env_;
#[cfg(feature = "slim")]
pub mod find;
#[cfg(feature = "slim")]
pub mod grep;
#[cfg(feature = "slim")]
pub mod head;
#[cfg(feature = "slim")]
pub mod hostname_;
#[cfg(feature = "slim")]
pub mod ln;
#[cfg(feature = "slim")]
pub mod ls;
#[cfg(feature = "slim")]
pub mod mkdir;
#[cfg(feature = "slim")]
pub mod mv;
#[cfg(feature = "slim")]
pub mod printf;
#[cfg(feature = "slim")]
pub mod pwd;
#[cfg(feature = "slim")]
pub mod realpath;
#[cfg(feature = "slim")]
pub mod rm;
#[cfg(feature = "slim")]
pub mod sed;
#[cfg(feature = "slim")]
pub mod seq;
#[cfg(feature = "slim")]
pub mod sleep_;
#[cfg(feature = "slim")]
pub mod sort;
#[cfg(feature = "slim")]
pub mod stat;
#[cfg(feature = "slim")]
pub mod tail;
#[cfg(feature = "slim")]
pub mod tee;
#[cfg(feature = "slim")]
pub mod touch;
#[cfg(feature = "slim")]
pub mod tr;
#[cfg(feature = "slim")]
pub mod uname;
#[cfg(feature = "slim")]
pub mod uniq;
#[cfg(feature = "slim")]
pub mod wc;
#[cfg(feature = "slim")]
pub mod which;
#[cfg(feature = "slim")]
pub mod whoami;
#[cfg(feature = "slim")]
pub mod xargs;

// --- Hashing ---------------------------------------------------------------

#[cfg(feature = "hashing")]
pub mod hashsum;

// --- Archives --------------------------------------------------------------

#[cfg(feature = "archives")]
pub mod gzip_;
#[cfg(feature = "archives")]
pub mod tar_;
#[cfg(feature = "archives")]
pub mod zip_;

// --- Disk usage ------------------------------------------------------------

#[cfg(feature = "disk")]
pub mod df;
#[cfg(feature = "disk")]
pub mod du;

// --- Network ---------------------------------------------------------------

#[cfg(feature = "network")]
pub mod dig;
#[cfg(feature = "network")]
pub mod http;
#[cfg(feature = "network")]
pub mod nc;

// --- JSON ------------------------------------------------------------------

#[cfg(feature = "json")]
pub mod jq;

// --- Extras (full-only) -----------------------------------------------------

#[cfg(feature = "extras")]
pub mod base64;
#[cfg(feature = "extras")]
pub mod cmp;
#[cfg(feature = "extras")]
pub mod column;
#[cfg(feature = "extras")]
pub mod comm;
#[cfg(feature = "extras")]
pub mod dd;
#[cfg(feature = "extras")]
pub mod diff;
#[cfg(feature = "extras")]
pub mod expand;
#[cfg(feature = "extras")]
pub mod fmt;
#[cfg(feature = "extras")]
pub mod fold;
#[cfg(feature = "extras")]
pub mod getopt;
#[cfg(feature = "extras")]
pub mod groups;
#[cfg(feature = "extras")]
pub mod hexdump;
#[cfg(feature = "extras")]
pub mod id;
#[cfg(feature = "extras")]
pub mod join;
#[cfg(feature = "extras")]
pub mod mktemp;
#[cfg(feature = "extras")]
pub mod nl;
#[cfg(feature = "extras")]
pub mod od;
#[cfg(feature = "extras")]
pub mod paste;
#[cfg(feature = "extras")]
pub mod rev;
#[cfg(feature = "extras")]
pub mod split;
#[cfg(feature = "extras")]
pub mod tac;
#[cfg(feature = "extras")]
pub mod truncate;
#[cfg(feature = "extras")]
pub mod unexpand;
#[cfg(feature = "extras")]
pub mod yes;

// --- Build the table -------------------------------------------------------

/// All registered applets. Aliases are handled by the registry and don't
/// appear here.
pub const ALL: &[Applet] = &[
    #[cfg(feature = "slim")]
    awk::APPLET,
    #[cfg(feature = "slim")]
    basename::APPLET,
    #[cfg(feature = "slim")]
    bools::TRUE,
    #[cfg(feature = "slim")]
    bools::FALSE,
    #[cfg(feature = "slim")]
    cat::APPLET,
    #[cfg(feature = "slim")]
    chmod::APPLET,
    #[cfg(feature = "slim")]
    cp::APPLET,
    #[cfg(feature = "slim")]
    cut::APPLET,
    #[cfg(feature = "slim")]
    date::APPLET,
    #[cfg(feature = "slim")]
    dirname::APPLET,
    #[cfg(feature = "slim")]
    echo::APPLET,
    #[cfg(feature = "slim")]
    env_::APPLET,
    #[cfg(feature = "slim")]
    find::APPLET,
    #[cfg(feature = "slim")]
    grep::APPLET,
    #[cfg(feature = "slim")]
    head::APPLET,
    #[cfg(feature = "slim")]
    hostname_::APPLET,
    #[cfg(feature = "slim")]
    ln::APPLET,
    #[cfg(feature = "slim")]
    ls::APPLET,
    #[cfg(feature = "slim")]
    mkdir::APPLET,
    #[cfg(feature = "slim")]
    mv::APPLET,
    #[cfg(feature = "slim")]
    printf::APPLET,
    #[cfg(feature = "slim")]
    pwd::APPLET,
    #[cfg(feature = "slim")]
    realpath::APPLET,
    #[cfg(feature = "slim")]
    rm::APPLET,
    #[cfg(feature = "slim")]
    sed::APPLET,
    #[cfg(feature = "slim")]
    seq::APPLET,
    #[cfg(feature = "slim")]
    sleep_::APPLET,
    #[cfg(feature = "slim")]
    sort::APPLET,
    #[cfg(feature = "slim")]
    stat::APPLET,
    #[cfg(feature = "slim")]
    tail::APPLET,
    #[cfg(feature = "slim")]
    tee::APPLET,
    #[cfg(feature = "slim")]
    touch::APPLET,
    #[cfg(feature = "slim")]
    tr::APPLET,
    #[cfg(feature = "slim")]
    uname::APPLET,
    #[cfg(feature = "slim")]
    uniq::APPLET,
    #[cfg(feature = "slim")]
    wc::APPLET,
    #[cfg(feature = "slim")]
    which::APPLET,
    #[cfg(feature = "slim")]
    whoami::APPLET,
    #[cfg(feature = "slim")]
    xargs::APPLET,
    #[cfg(feature = "hashing")]
    hashsum::MD5,
    #[cfg(feature = "hashing")]
    hashsum::SHA1,
    #[cfg(feature = "hashing")]
    hashsum::SHA256,
    #[cfg(feature = "hashing")]
    hashsum::SHA512,
    #[cfg(feature = "archives")]
    gzip_::GZIP,
    #[cfg(feature = "archives")]
    gzip_::GUNZIP,
    #[cfg(feature = "archives")]
    tar_::APPLET,
    #[cfg(feature = "archives")]
    zip_::ZIP,
    #[cfg(feature = "archives")]
    zip_::UNZIP,
    #[cfg(feature = "disk")]
    df::APPLET,
    #[cfg(feature = "disk")]
    du::APPLET,
    #[cfg(feature = "network")]
    dig::APPLET,
    #[cfg(feature = "network")]
    http::APPLET,
    #[cfg(feature = "network")]
    nc::APPLET,
    #[cfg(feature = "json")]
    jq::APPLET,
    #[cfg(feature = "extras")]
    base64::APPLET,
    #[cfg(feature = "extras")]
    cmp::APPLET,
    #[cfg(feature = "extras")]
    column::APPLET,
    #[cfg(feature = "extras")]
    comm::APPLET,
    #[cfg(feature = "extras")]
    dd::APPLET,
    #[cfg(feature = "extras")]
    diff::APPLET,
    #[cfg(feature = "extras")]
    expand::APPLET,
    #[cfg(feature = "extras")]
    fmt::APPLET,
    #[cfg(feature = "extras")]
    fold::APPLET,
    #[cfg(feature = "extras")]
    getopt::APPLET,
    #[cfg(feature = "extras")]
    groups::APPLET,
    #[cfg(feature = "extras")]
    hexdump::APPLET,
    #[cfg(feature = "extras")]
    id::APPLET,
    #[cfg(feature = "extras")]
    join::APPLET,
    #[cfg(feature = "extras")]
    od::APPLET,
    #[cfg(feature = "extras")]
    paste::APPLET,
    #[cfg(feature = "extras")]
    split::APPLET,
    #[cfg(feature = "extras")]
    mktemp::APPLET,
    #[cfg(feature = "extras")]
    nl::APPLET,
    #[cfg(feature = "extras")]
    rev::APPLET,
    #[cfg(feature = "extras")]
    tac::APPLET,
    #[cfg(feature = "extras")]
    truncate::APPLET,
    #[cfg(feature = "extras")]
    unexpand::APPLET,
    #[cfg(feature = "extras")]
    yes::APPLET,
];
