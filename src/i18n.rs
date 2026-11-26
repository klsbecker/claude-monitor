//! User-facing text in English, Brazilian Portuguese and Spanish.
//!
//! Every string the user can see lives in a [`Strings`] table. Templates use named placeholders such as
//! `{pct}`, filled with [`fill`]. To add a language, add a [`Lang`] variant and a table.

use std::sync::atomic::{AtomicU8, Ordering};

use windows::Win32::Globalization::GetUserDefaultUILanguage;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    En,
    Pt,
    Es,
}

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Pt, Lang::Es];

    /// The value saved in the registry (0 means "follow Windows").
    pub fn code(self) -> u32 {
        match self {
            Lang::En => 1,
            Lang::Pt => 2,
            Lang::Es => 3,
        }
    }

    pub fn from_code(code: u32) -> Option<Lang> {
        Lang::ALL.into_iter().find(|l| l.code() == code)
    }

    /// The language's own name, as listed in the menu.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Pt => "Português (Brasil)",
            Lang::Es => "Español",
        }
    }

    /// The Windows display language, or English when it has no translation.
    pub fn system() -> Lang {
        const LANG_PORTUGUESE: u16 = 0x16;
        const LANG_SPANISH: u16 = 0x0A;
        match unsafe { GetUserDefaultUILanguage() } & 0x3FF {
            LANG_PORTUGUESE => Lang::Pt,
            LANG_SPANISH => Lang::Es,
            _ => Lang::En,
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang.code() as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    Lang::from_code(CURRENT.load(Ordering::Relaxed) as u32).unwrap_or(Lang::En)
}

/// The text table of the current language.
pub fn tr() -> &'static Strings {
    match lang() {
        Lang::En => &EN,
        Lang::Pt => &PT,
        Lang::Es => &ES,
    }
}

/// `template` with each `{name}` replaced by its value.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = template.to_owned();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

pub struct Strings {
    pub weekdays: [&'static str; 7],
    /// Dates as month/day instead of day/month.
    pub month_first: bool,

    // Limits.
    pub session_5h: &'static str,
    pub session: &'static str,
    pub week: &'static str,
    pub week_opus: &'static str,
    pub week_sonnet: &'static str,

    // Errors shown on the card.
    pub not_signed_in: &'static str,
    pub sign_in_expired: &'static str,
    pub token_rejected: &'static str,
    pub no_limits: &'static str,
    pub error: &'static str,

    // Card and menu header.
    pub at_time: &'static str,
    pub resets: &'static str,
    pub forecast_value: &'static str,
    pub now: &'static str,
    pub pace_full: &'static str,
    pub pace_at_reset: &'static str,
    pub detail_resets: &'static str,
    pub checked_at: &'static str,
    pub loading: &'static str,

    // Notifications.
    pub warn_title: &'static str,
    pub warn_body: &'static str,
    pub warn_resets: &'static str,
    pub reset_title: &'static str,
    pub reset_body: &'static str,

    // Dialogs.
    pub login_install: &'static str,
    pub login_sign_in: &'static str,
    pub clear_history_confirm: &'static str,

    // Menu.
    pub menu_web: &'static str,
    pub menu_refresh: &'static str,
    pub menu_interval: &'static str,
    pub one_minute: &'static str,
    pub minutes: &'static str,
    pub menu_show: &'static str,
    pub default_suffix: &'static str,
    pub menu_red_from: &'static str,
    pub never: &'static str,
    pub menu_warn_never: &'static str,
    pub menu_warn_at: &'static str,
    pub menu_notify_reset: &'static str,
    pub menu_tray_icon: &'static str,
    pub menu_red_icon: &'static str,
    pub menu_icon_color: &'static str,
    pub color_taskbar: &'static str,
    pub color_claude: &'static str,
    pub color_presets: [&'static str; 6],
    pub color_custom: &'static str,
    pub menu_background: &'static str,
    pub menu_position: &'static str,
    pub position_default: &'static str,
    pub px_left: &'static str,
    pub move_left: &'static str,
    pub move_right: &'static str,
    pub menu_height: &'static str,
    pub centered: &'static str,
    pub px_up: &'static str,
    pub px_down: &'static str,
    pub move_up: &'static str,
    pub move_down: &'static str,
    pub custom: &'static str,
    pub menu_config_dir: &'static str,
    pub config_default: &'static str,
    pub config_choose: &'static str,
    pub menu_language: &'static str,
    pub language_auto: &'static str,
    pub menu_sign_in: &'static str,
    pub menu_autostart: &'static str,
    pub menu_clear_history: &'static str,
    pub menu_about: &'static str,
    pub menu_quit: &'static str,
}

pub static EN: Strings = Strings {
    weekdays: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    month_first: true,

    session_5h: "Session (5h)",
    session: "Session",
    week: "Week",
    week_opus: "Week (Opus)",
    week_sonnet: "Week (Sonnet)",

    not_signed_in: "Claude Code is not signed in",
    sign_in_expired: "the Claude Code sign-in has expired",
    token_rejected: "token rejected (HTTP 401)",
    no_limits: "the response has no limits",
    error: "Error: {error}",

    at_time: "at {time}",
    resets: "resets {when}",
    forecast_value: "~{pct}% forecast",
    now: "now",
    pace_full: "At this pace, reaches 100% {when}",
    pace_at_reset: "At this pace, ~{pct}% at the reset",
    detail_resets: "(resets {when})",
    checked_at: "Checked at {time}",
    loading: "Loading…",

    warn_title: "{label}: {pct}% of the limit",
    warn_body: "Passed {level}%.",
    warn_resets: "Resets {when}.",
    reset_title: "{label}: limit reset",
    reset_body: "Usage is back to zero.",

    login_install: "Claude Monitor reads your usage through the Claude Code sign-in, and Claude Code is not installed.\n\n\
                    Install Claude Code and sign in now?\n\
                    PowerShell opens with the official installer (claude.ai/install.ps1), then the sign-in page opens in your browser.",
    login_sign_in: "Claude Monitor reads your usage through the Claude Code sign-in, which is not active.\n\n\
                    Sign in now? PowerShell opens the Claude Code sign-in in your browser.",
    clear_history_confirm: "Delete all the usage history saved by Claude Monitor? The charts start again empty.",

    menu_web: "View usage on claude.ai",
    menu_refresh: "Refresh now",
    menu_interval: "Refresh every",
    one_minute: "1 minute",
    minutes: "{n} minutes",
    menu_show: "Show on the taskbar",
    default_suffix: "  (default)",
    menu_red_from: "Red from {pct}%",
    never: "Never",
    menu_warn_never: "Notify: never",
    menu_warn_at: "Notify at {pct}%",
    menu_notify_reset: "Notify when it resets",
    menu_tray_icon: "Show as a notification-area icon (Windows arranges it)",
    menu_red_icon: "Red icon at the alert level",
    menu_icon_color: "Icon color",
    color_taskbar: "Taskbar color",
    color_claude: "Claude orange",
    color_presets: ["Green", "Blue", "Cyan", "Purple", "Pink", "Orange"],
    color_custom: "Custom color…",
    menu_background: "Background behind the text (for translucent taskbars)",
    menu_position: "Position on the taskbar",
    position_default: "Next to the notification area",
    px_left: "{px} px to the left",
    move_left: "Move 10 px left",
    move_right: "Move 10 px right",
    menu_height: "Height on the taskbar",
    centered: "Centered",
    px_up: "{px} px up",
    px_down: "{px} px down",
    move_up: "Move 10 px up",
    move_down: "Move 10 px down",
    custom: "Custom: {value}",
    menu_config_dir: "Claude Code folder",
    config_default: "Default: {path}",
    config_choose: "Choose folder…",
    menu_language: "Language",
    language_auto: "Automatic (Windows)",
    menu_sign_in: "Sign in to Claude Code…",
    menu_autostart: "Start with Windows",
    menu_clear_history: "Delete usage history…",
    menu_about: "About Claude Monitor {version}",
    menu_quit: "Quit",
};

pub static PT: Strings = Strings {
    weekdays: ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"],
    month_first: false,

    session_5h: "Sessão (5h)",
    session: "Sessão",
    week: "Semana",
    week_opus: "Semana (Opus)",
    week_sonnet: "Semana (Sonnet)",

    not_signed_in: "Claude Code sem login",
    sign_in_expired: "o login do Claude Code expirou",
    token_rejected: "token recusado (HTTP 401)",
    no_limits: "resposta sem limites",
    error: "Erro: {error}",

    at_time: "às {time}",
    resets: "reseta {when}",
    forecast_value: "~{pct}% previsto",
    now: "agora",
    pace_full: "No ritmo atual, chega a 100% {when}",
    pace_at_reset: "No ritmo atual, ~{pct}% no reset",
    detail_resets: "(reseta {when})",
    checked_at: "Consultado às {time}",
    loading: "Carregando…",

    warn_title: "{label}: {pct}% do limite",
    warn_body: "Passou de {level}%.",
    warn_resets: "Reseta {when}.",
    reset_title: "{label}: limite resetado",
    reset_body: "O uso voltou a zero.",

    login_install: "O Claude Monitor lê o seu uso pelo login do Claude Code, que não está instalado.\n\n\
                    Instalar o Claude Code e entrar na sua conta agora?\n\
                    Vai abrir o PowerShell com o instalador oficial (claude.ai/install.ps1) e depois o login no navegador.",
    login_sign_in: "O Claude Monitor lê o seu uso pelo login do Claude Code, que não está ativo.\n\n\
                    Entrar na sua conta agora? Vai abrir o PowerShell com o login do Claude Code no navegador.",
    clear_history_confirm: "Apagar todo o histórico de uso guardado pelo Claude Monitor? Os gráficos recomeçam vazios.",

    menu_web: "Ver uso no claude.ai",
    menu_refresh: "Atualizar agora",
    menu_interval: "Atualizar a cada",
    one_minute: "1 minuto",
    minutes: "{n} minutos",
    menu_show: "Mostrar na barra de tarefas",
    default_suffix: "  (padrão)",
    menu_red_from: "Vermelho a partir de {pct}%",
    never: "Nunca",
    menu_warn_never: "Avisar: nunca",
    menu_warn_at: "Avisar ao chegar em {pct}%",
    menu_notify_reset: "Avisar quando resetar",
    menu_tray_icon: "Mostrar como ícone da área de notificação (o Windows organiza)",
    menu_red_icon: "Ícone vermelho no nível de alerta",
    menu_icon_color: "Cor do ícone",
    color_taskbar: "Cor da barra de tarefas",
    color_claude: "Laranja do Claude",
    color_presets: ["Verde", "Azul", "Ciano", "Roxo", "Rosa", "Laranja"],
    color_custom: "Cor personalizada…",
    menu_background: "Fundo atrás do texto (para barras translúcidas)",
    menu_position: "Posição na barra de tarefas",
    position_default: "Ao lado da área de notificação",
    px_left: "{px} px à esquerda",
    move_left: "Mover 10 px para a esquerda",
    move_right: "Mover 10 px para a direita",
    menu_height: "Altura na barra de tarefas",
    centered: "Centralizado",
    px_up: "{px} px para cima",
    px_down: "{px} px para baixo",
    move_up: "Mover 10 px para cima",
    move_down: "Mover 10 px para baixo",
    custom: "Personalizada: {value}",
    menu_config_dir: "Pasta do Claude Code",
    config_default: "Padrão: {path}",
    config_choose: "Escolher pasta…",
    menu_language: "Idioma",
    language_auto: "Automático (Windows)",
    menu_sign_in: "Entrar no Claude Code…",
    menu_autostart: "Iniciar com o Windows",
    menu_clear_history: "Apagar histórico de uso…",
    menu_about: "Sobre o Claude Monitor {version}",
    menu_quit: "Sair",
};

pub static ES: Strings = Strings {
    weekdays: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
    month_first: false,

    session_5h: "Sesión (5h)",
    session: "Sesión",
    week: "Semana",
    week_opus: "Semana (Opus)",
    week_sonnet: "Semana (Sonnet)",

    not_signed_in: "Claude Code no tiene sesión iniciada",
    sign_in_expired: "la sesión de Claude Code caducó",
    token_rejected: "token rechazado (HTTP 401)",
    no_limits: "la respuesta no trae límites",
    error: "Error: {error}",

    at_time: "a las {time}",
    resets: "se reinicia {when}",
    forecast_value: "~{pct}% previsto",
    now: "ahora",
    pace_full: "A este ritmo, llega al 100% {when}",
    pace_at_reset: "A este ritmo, ~{pct}% al reiniciarse",
    detail_resets: "(se reinicia {when})",
    checked_at: "Consultado a las {time}",
    loading: "Cargando…",

    warn_title: "{label}: {pct}% del límite",
    warn_body: "Superó el {level}%.",
    warn_resets: "Se reinicia {when}.",
    reset_title: "{label}: límite reiniciado",
    reset_body: "El uso volvió a cero.",

    login_install: "Claude Monitor lee tu uso a través de la sesión de Claude Code, que no está instalado.\n\n\
                    ¿Instalar Claude Code e iniciar sesión ahora?\n\
                    Se abrirá PowerShell con el instalador oficial (claude.ai/install.ps1) y luego el inicio de sesión en el navegador.",
    login_sign_in: "Claude Monitor lee tu uso a través de la sesión de Claude Code, que no está activa.\n\n\
                    ¿Iniciar sesión ahora? Se abrirá PowerShell con el inicio de sesión de Claude Code en el navegador.",
    clear_history_confirm: "¿Borrar todo el historial de uso guardado por Claude Monitor? Los gráficos vuelven a empezar vacíos.",

    menu_web: "Ver uso en claude.ai",
    menu_refresh: "Actualizar ahora",
    menu_interval: "Actualizar cada",
    one_minute: "1 minuto",
    minutes: "{n} minutos",
    menu_show: "Mostrar en la barra de tareas",
    default_suffix: "  (predeterminado)",
    menu_red_from: "Rojo desde {pct}%",
    never: "Nunca",
    menu_warn_never: "Avisar: nunca",
    menu_warn_at: "Avisar al llegar a {pct}%",
    menu_notify_reset: "Avisar cuando se reinicie",
    menu_tray_icon: "Mostrar como icono del área de notificación (Windows lo ordena)",
    menu_red_icon: "Icono rojo en el nivel de alerta",
    menu_icon_color: "Color del icono",
    color_taskbar: "Color de la barra de tareas",
    color_claude: "Naranja de Claude",
    color_presets: ["Verde", "Azul", "Cian", "Morado", "Rosa", "Naranja"],
    color_custom: "Color personalizado…",
    menu_background: "Fondo detrás del texto (para barras translúcidas)",
    menu_position: "Posición en la barra de tareas",
    position_default: "Junto al área de notificación",
    px_left: "{px} px a la izquierda",
    move_left: "Mover 10 px a la izquierda",
    move_right: "Mover 10 px a la derecha",
    menu_height: "Altura en la barra de tareas",
    centered: "Centrado",
    px_up: "{px} px hacia arriba",
    px_down: "{px} px hacia abajo",
    move_up: "Mover 10 px hacia arriba",
    move_down: "Mover 10 px hacia abajo",
    custom: "Personalizada: {value}",
    menu_config_dir: "Carpeta de Claude Code",
    config_default: "Predeterminada: {path}",
    config_choose: "Elegir carpeta…",
    menu_language: "Idioma",
    language_auto: "Automático (Windows)",
    menu_sign_in: "Iniciar sesión en Claude Code…",
    menu_autostart: "Iniciar con Windows",
    menu_clear_history: "Borrar historial de uso…",
    menu_about: "Acerca de Claude Monitor {version}",
    menu_quit: "Salir",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_replaces_every_placeholder() {
        assert_eq!(fill("{label}: {pct}% ({pct})", &[("label", "Week"), ("pct", "42")]), "Week: 42% (42)");
    }

    #[test]
    fn codes_round_trip() {
        for l in Lang::ALL {
            assert_eq!(Lang::from_code(l.code()), Some(l));
        }
        assert_eq!(Lang::from_code(0), None);
    }
}
