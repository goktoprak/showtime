use leptos::prelude::*;

/// How a message reads, which is also how it's coloured.
///
/// `Info` is not a weaker `Success`: the refresh-all result is reported with
/// it because that message can carry failures ("Refreshed 3 shows. 1
/// failed."), so it would be wrong in green.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Error,
    Success,
    Info,
}

/// Something to tell the user about the last thing they did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub tone: Tone,
}

impl Message {
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Error,
        }
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Success,
        }
    }

    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Info,
        }
    }

    fn class(&self) -> &'static str {
        match self.tone {
            Tone::Error => "msg error",
            Tone::Success => "msg success",
            Tone::Info => "msg",
        }
    }
}

/// Inline page message. Renders nothing when there is none.
///
/// Replaces the `alert()` calls the show page used to make, so that every
/// page reports success and failure the same way.
#[component]
pub fn Notice(#[prop(into)] message: Signal<Option<Message>>) -> impl IntoView {
    move || {
        message
            .get()
            .map(|m| view! { <div class=m.class()>{m.text.clone()}</div> })
    }
}
