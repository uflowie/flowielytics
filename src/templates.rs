use askama::Template;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub site_name: String,
    pub query_string: String,
}

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

impl<T: Template> SseTemplate for T
{
    fn render_sse(&self) -> askama::Result<String> {
        self.render()
            .map(|x| x.chars().filter(|&c| c != '\n' && c != '\r').collect())
    }
}
