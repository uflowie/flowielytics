use askama::Template;

#[derive(Template)]
#[template(path = "not-connected.html")]
pub struct NotConnectedTemplate;

#[derive(Template)]
#[template(path = "connected.html")]
pub struct ConnectedTemplate;

#[derive(Template)]
#[template(path = "playing.html")]
pub struct PlayingTemplate {
    pub url: String,
}

pub trait SseTemplate {
    fn render_sse(&self) -> askama::Result<String>;
}

impl<T> SseTemplate for T
where
    T: Template,
{
    fn render_sse(&self) -> askama::Result<String> {
        self.render()
            .map(|x| x.chars().filter(|&c| c != '\n' && c != '\r').collect())
    }
}
