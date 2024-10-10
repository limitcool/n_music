use crate::Localization;
use serde::{Deserialize, Serialize};
use slint::{SharedString, VecModel};

include!(concat!(env!("OUT_DIR"), "/localizations.rs"));

#[derive(Debug, Deserialize, Serialize)]
pub struct Locale {
    settings: Option<String>,
    search: Option<String>,
    theme: Option<String>,
    window_size: Option<String>,
    music_path: Option<String>,
    language: Option<String>,
}

pub fn localize(denominator: Option<String>, localization: Localization) {
    let denominator = denominator.unwrap_or(
        sys_locale::get_locale()
            .unwrap()
            .split('-')
            .next()
            .unwrap()
            .to_string(),
    );
    let locale = get_locale(&denominator);
    let english = get_locale("en");

    let mut localizations = LOCALIZATIONS
        .iter()
        .map(|(_, name)| name.to_string())
        .map(|name| name.into())
        .collect::<Vec<SharedString>>();
    localizations.sort();
    localization.set_localizations(VecModel::from_slice(&localizations));
    localization.set_current_locale(get_locale_name(Some(&denominator)).into());
    localization.set_settings(
        locale
            .settings
            .as_ref()
            .unwrap_or(english.settings.as_ref().unwrap())
            .into(),
    );
    localization.set_search(
        locale
            .search
            .as_ref()
            .unwrap_or(english.search.as_ref().unwrap())
            .into(),
    );
    localization.set_theme(
        locale
            .theme
            .as_ref()
            .unwrap_or(english.theme.as_ref().unwrap())
            .into(),
    );
    localization.set_window_size(
        locale
            .window_size
            .as_ref()
            .unwrap_or(english.window_size.as_ref().unwrap())
            .into(),
    );
    localization.set_music_path(
        locale
            .music_path
            .as_ref()
            .unwrap_or(english.music_path.as_ref().unwrap())
            .into(),
    );
    localization.set_language(
        locale
            .language
            .as_ref()
            .unwrap_or(english.language.as_ref().unwrap())
            .into(),
    );
}

pub fn get_locale_name(denominator: Option<&str>) -> &str {
    if let Some(denominator) = denominator {
        for localization in LOCALIZATIONS {
            if denominator == localization.0 {
                return localization.1;
            }
        }
    }
    "English"
}

pub fn get_locale_denominator(name: Option<&str>) -> &str {
    if let Some(name) = name {
        for localization in LOCALIZATIONS {
            if name == localization.1 {
                return localization.0;
            }
        }
    }
    "en"
}
