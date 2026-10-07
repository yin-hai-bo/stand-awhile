use windows::Win32::Globalization::GetUserDefaultUILanguage;

const LANG_CHINESE_PRIMARY_ID: u16 = 0x04;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Chinese,
    English,
}

pub fn detect_language() -> Language {
    let lang_id = unsafe { GetUserDefaultUILanguage() };
    detect_language_from_lang_id(lang_id)
}

pub fn resolve_language(configured_language: &str) -> Language {
    let language = configured_language.trim().to_ascii_lowercase();
    if language.is_empty() || language == "auto" {
        detect_language()
    } else if language == "zh" || language.starts_with("zh-") {
        Language::Chinese
    } else {
        Language::English
    }
}

pub fn main_window_title(language: Language) -> &'static str {
    match language {
        Language::Chinese => "站一站",
        Language::English => "Stand Awhile",
    }
}

pub fn tray_tooltip(language: Language, running: bool) -> &'static str {
    match (language, running) {
        (Language::Chinese, true) => "站一站（计时中）",
        (Language::English, true) => "Stand Awhile (Timing)",
        (_, false) => main_window_title(language),
    }
}

pub fn existing_instance_unavailable_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "程序已在运行，但未能找到主窗口。请稍后重试。",
        Language::English => "The app is already running, but its main window could not be found. Please try again.",
    }
}

pub fn pet_reminder_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "该站起来活动一下啦！\n单击桌宠，开始新一轮倒计时",
        Language::English => "Time to stand up and stretch!\nClick the pet to start a new countdown.",
    }
}

pub fn timer_hint_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "倒计时结束后，小伙伴会提醒你起来活动活动。",
        Language::English => "When the countdown ends, your pet will remind you to get up and stretch.",
    }
}

pub fn auto_hide_text(language: Language) -> &'static str {
    match language {
        Language::Chinese => "开始倒计时后自动隐藏主窗口",
        Language::English => "Auto-hide main window when starting the timer",
    }
}

fn detect_language_from_lang_id(lang_id: u16) -> Language {
    let primary_language = lang_id & 0x03ff;
    if primary_language == LANG_CHINESE_PRIMARY_ID {
        Language::Chinese
    } else {
        Language::English
    }
}

#[cfg(test)]
mod tests {
    use super::{Language, detect_language_from_lang_id, resolve_language};

    #[test]
    fn tray_tooltips_follow_language_and_running_state() {
        assert_eq!(super::tray_tooltip(Language::Chinese, true), "站一站（计时中）");
        assert_eq!(super::tray_tooltip(Language::English, true), "Stand Awhile (Timing)");
        for language in [Language::Chinese, Language::English] {
            assert_eq!(super::tray_tooltip(language, false), super::main_window_title(language));
        }
    }

    #[test]
    fn detects_chinese_as_chinese() {
        assert_eq!(detect_language_from_lang_id(0x0804), Language::Chinese);
        assert_eq!(detect_language_from_lang_id(0x0404), Language::Chinese);
    }

    #[test]
    fn falls_back_to_english_for_non_chinese_languages() {
        assert_eq!(detect_language_from_lang_id(0x0409), Language::English);
        assert_eq!(detect_language_from_lang_id(0x0411), Language::English);
    }

    #[test]
    fn resolves_configured_chinese_language() {
        assert_eq!(resolve_language("zh"), Language::Chinese);
        assert_eq!(resolve_language("zh-CN"), Language::Chinese);
    }

    #[test]
    fn resolves_configured_non_chinese_language_to_english() {
        assert_eq!(resolve_language("en"), Language::English);
        assert_eq!(resolve_language("ja-JP"), Language::English);
    }
}
