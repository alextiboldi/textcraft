//! Translations (craft-i18n; English source text is the key).
include!(concat!(env!("OUT_DIR"), "/craft_i18n.rs")); // CRAFT_I18N, built by build.rs

pub static I18N: std::sync::LazyLock<craft_i18n::I18n> = std::sync::LazyLock::new(|| {
    // ponytail: offers every embedded left-to-right locale; TextCraft has no right-to-left layout yet.
    let offered: Vec<&str> = CRAFT_I18N.iter().map(|l| l.0).filter(|c| craft_i18n::direction(c) != craft_i18n::Direction::Rtl).collect();
    let (i18n, problems) = craft_i18n::I18n::new(CRAFT_I18N, &offered);
    problems.iter().for_each(|p| eprintln!("translation catalog: {p}"));
    i18n
});

thread_local! { pub static CURRENT: std::cell::Cell<craft_i18n::Lang> = const { std::cell::Cell::new(craft_i18n::Lang::EN) }; }

/// English `$s` in the interface language.
macro_rules! tl {
    ($s:expr) => {
        $crate::i18n::I18N.t($crate::i18n::CURRENT.get(), $s)
    };
}
/// A command's label by its id.
macro_rules! tid {
    ($id:expr, $label:expr) => {
        $crate::i18n::I18N.tid($crate::i18n::CURRENT.get(), $id, $label)
    };
}
/// A count's wording, `{n}` filled: `trn!(n, "{n} match", "{n} matches")`.
macro_rules! trn {
    ($n:expr, $one:expr, $other:expr) => {
        $crate::i18n::I18N.trn($crate::i18n::CURRENT.get(), $n as u64, $one, $other)
    };
}
/// `{name}` placeholders of a translated message filled in.
macro_rules! tf { ($s:expr, $($k:ident = $v:expr),+ $(,)?) => { craft_i18n::fmt(tl!($s), &[$((stringify!($k), &$v.to_string())),+]) }; }

/// A number as the region writes it, isolated left to right.
pub fn num(n: usize) -> String {
    craft_i18n::ltr(&I18N.format().integer(n as u64))
}
