//! The system's preferred interface language, for text the native side shows before (or without)
//! a window — the menu-bar / notification-area menu (fabric-workspace knowledge/localization.md,
//! L10N-01). The window may override it with the person's choice.

/// Whether a language tag (`ru`, `ru-RU`, `ru_RU.UTF-8`) is Russian.
pub fn is_russian_tag(tag: &str) -> bool {
    let tag = tag.trim().to_ascii_lowercase();
    tag == "ru" || tag.starts_with("ru-") || tag.starts_with("ru_") || tag.starts_with("ru.")
}

/// Whether the first preferred language of this user is Russian.
pub fn system_prefers_russian() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos_first_language().is_some_and(|tag| is_russian_tag(&tag))
    }
    #[cfg(windows)]
    {
        // LANG_RUSSIAN is primary language 0x19 (the low ten bits of the LANGID).
        let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
        id & 0x3ff == 0x19
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .is_some_and(|v| is_russian_tag(&v))
    }
}

#[cfg(target_os = "macos")]
fn macos_first_language() -> Option<String> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> CFArrayRef;
    }
    // SAFETY: a Copy function returns a +1 array (or null); wrap_under_create_rule takes it over.
    let languages = unsafe { CFLocaleCopyPreferredLanguages() };
    if languages.is_null() {
        return None;
    }
    let languages: CFArray<CFString> = unsafe { CFArray::wrap_under_create_rule(languages) };
    languages.get(0).map(|first| first.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn russian_tags_are_recognised_and_others_are_not() {
        for tag in ["ru", "ru-RU", "RU-ru", "ru_RU.UTF-8", " ru "] {
            assert!(is_russian_tag(tag), "{tag}");
        }
        for tag in ["", "en-US", "uk-UA", "rus", "be-RU"] {
            assert!(!is_russian_tag(tag), "{tag}");
        }
    }
    #[test]
    fn the_system_language_is_read_without_failing() {
        // Whatever this machine prefers, the read returns instead of panicking.
        let _ = system_prefers_russian();
    }
}
