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

pub fn main_window_title(language: Language) -> &'static str {
    match language {
        Language::Chinese => "站一站",
        Language::English => "Stand Awhile",
    }
}

pub fn reminder_notification_title(language: Language) -> &'static str {
    match language {
        Language::Chinese => "时间到了",
        Language::English => "Time is up",
    }
}

pub fn reminder_notification_message(language: Language) -> &'static str {
    match language {
        Language::Chinese => "起来活动一下吧。",
        Language::English => "Time to stand up and move around.",
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
    use super::{Language, detect_language_from_lang_id};

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
}
