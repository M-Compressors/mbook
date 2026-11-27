//! What an agent is told: the skill `mbook-authoring`, served as a skill of
//! MCP (SEP-2640), as resources, as the topics of `get_guide`, and as the
//! instructions of `initialize` (mbook-ui docs, "What the agent is told").
//!
//! The files are part of the binary, so every way gives the same text.

use rmcp::model::{
    Resource, ResourceContents, Skill, SkillFrontmatter, SkillResource, SkillResources,
};

/// The name of the skill.
pub(crate) const NAME: &str = "mbook-authoring";

/// The instructions of `initialize`.
pub(crate) const INSTRUCTIONS: &str = include_str!("../../../assets/mcp/instructions.md");

/// One file of the skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct File {
    /// The path in the skill.
    pub(crate) path: &'static str,
    /// The topic of `get_guide` that gives it.
    pub(crate) topic: &'static str,
    /// The text.
    pub(crate) text: &'static str,
}

impl File {
    /// The resource URI of the file.
    pub(crate) fn uri(self) -> String {
        format!("skill://{NAME}/{}", self.path)
    }
}

/// Every file of the skill; `SKILL.md` first.
pub(crate) const FILES: [File; 6] = [
    File {
        path: "SKILL.md",
        topic: "overview",
        text: include_str!("../../../assets/mcp/mbook-authoring/SKILL.md"),
    },
    File {
        path: "references/dialect.md",
        topic: "dialect",
        text: include_str!("../../../assets/mcp/mbook-authoring/references/dialect.md"),
    },
    File {
        path: "references/figures.md",
        topic: "figures",
        text: include_str!("../../../assets/mcp/mbook-authoring/references/figures.md"),
    },
    File {
        path: "references/pictures.md",
        topic: "pictures",
        text: include_str!("../../../assets/mcp/mbook-authoring/references/pictures.md"),
    },
    File {
        path: "references/structure.md",
        topic: "structure",
        text: include_str!("../../../assets/mcp/mbook-authoring/references/structure.md"),
    },
    File {
        path: "references/errors.md",
        topic: "errors",
        text: include_str!("../../../assets/mcp/mbook-authoring/references/errors.md"),
    },
];

/// The topics of `get_guide`, in the order of the files.
pub(crate) fn topics() -> impl Iterator<Item = &'static str> {
    FILES.iter().map(|file| file.topic)
}

/// The text of a topic, without the front matter of `SKILL.md`.
pub(crate) fn guide(topic: &str) -> Option<&'static str> {
    FILES
        .iter()
        .find(|file| file.topic == topic)
        .map(|file| body(file.text))
}

/// A Markdown text without the front matter at its start. The lines may end
/// in a line feed or, as a checkout on Windows may leave them, in a carriage
/// return and a line feed.
fn body(text: &'static str) -> &'static str {
    let mut lines = text.split_inclusive('\n');
    if lines.next().map(str::trim_end) != Some("---") {
        return text;
    }
    let mut start = 0;
    let mut offset = 0;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        offset += line.len();
        if index > 0 && line.trim_end() == "---" {
            start = offset;
            break;
        }
    }
    if start == 0 {
        return text;
    }
    text[start..].trim_start_matches(['\r', '\n'])
}

/// The value of `key` in the front matter of a text.
fn front_matter<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return None;
    }
    lines
        .take_while(|line| *line != "---")
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
        .map(str::trim)
}

/// The description in the front matter of `SKILL.md`.
fn description() -> &'static str {
    front_matter(FILES[0].text, "description").unwrap_or_default()
}

/// The skill as `skills/list` and `skills/get` give it.
pub(crate) fn skill() -> Skill {
    Skill::new(
        FILES[0].uri(),
        SkillFrontmatter::new(NAME, description()),
        SkillResources::Files(
            FILES
                .iter()
                .map(|file| SkillResource::from_bytes(file.uri(), file.text.as_bytes()))
                .collect(),
        ),
    )
}

/// The files as resources.
pub(crate) fn resources() -> Vec<Resource> {
    FILES
        .iter()
        .map(|file| Resource::new(file.uri(), file.path).with_mime_type("text/markdown"))
        .collect()
}

/// The contents of the resource at `uri`, if it is a file of the skill.
pub(crate) fn read(uri: &str) -> Option<ResourceContents> {
    FILES
        .iter()
        .find(|file| file.uri() == uri)
        .map(|file| ResourceContents::text(file.text, uri))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_names_itself_and_describes_what_it_is_for() {
        assert_eq!(front_matter(FILES[0].text, "name"), Some(NAME));
        assert!(description().starts_with("Write and edit books"));
        assert_eq!(skill().uri, "skill://mbook-authoring/SKILL.md");
    }

    #[test]
    fn every_file_is_a_resource_that_can_be_read() {
        let resources = resources();
        assert_eq!(resources.len(), FILES.len());
        for resource in &resources {
            assert!(read(&resource.uri).is_some(), "{}", resource.uri);
        }
        assert!(read("skill://mbook-authoring/missing.md").is_none());
    }

    #[test]
    fn every_topic_gives_its_file_without_front_matter() {
        assert_eq!(
            topics().collect::<Vec<_>>(),
            [
                "overview",
                "dialect",
                "figures",
                "pictures",
                "structure",
                "errors"
            ]
        );
        let overview = guide("overview").unwrap();
        assert!(overview.starts_with("# Writing books in mbook"));
        assert!(guide("dialect").unwrap().starts_with("# What a chapter"));
        assert_eq!(guide("nothing"), None);
    }

    #[test]
    fn front_matter_is_found_with_windows_line_endings() {
        let text = "---\r\nname: x\r\ndescription: Does y\r\n---\r\n\r\n# Title\r\n";
        assert_eq!(body(text), "# Title\r\n");
        assert_eq!(front_matter(text, "description"), Some("Does y"));
    }

    #[test]
    fn a_text_without_front_matter_is_its_own_body() {
        assert_eq!(body("# Title\n"), "# Title\n");
        assert_eq!(body("---\nname: x\n"), "---\nname: x\n");
    }

    #[test]
    fn the_skill_names_every_topic_it_offers() {
        let overview = guide("overview").unwrap();
        for topic in topics().skip(1) {
            assert!(overview.contains(&format!("`{topic}`")), "{topic}");
        }
        assert!(INSTRUCTIONS.contains("get_guide"));
    }
}
