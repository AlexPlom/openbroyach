#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderLink {
    pub label: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Provider {
    pub id: String,
    pub display_name: String,
    pub links: Vec<ProviderLink>,
}

impl Provider {
    pub fn new(id: &str, display_name: &str) -> Self {
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            links: Vec::new(),
        }
    }

    pub fn with_links(id: &str, display_name: &str, links: Vec<(&str, &str)>) -> Self {
        Self {
            id: id.to_string(),
            display_name: display_name.to_string(),
            links: links
                .into_iter()
                .map(|(l, u)| ProviderLink {
                    label: l.to_string(),
                    url: u.to_string(),
                })
                .collect(),
        }
    }

    pub fn visible_links(&self) -> Vec<&ProviderLink> {
        self.links
            .iter()
            .filter(|link| {
                !link.label.trim().is_empty()
                    && !link.url.trim().is_empty()
                    && (link.url.starts_with("https://") || link.url.starts_with("http://"))
            })
            .collect()
    }
}
