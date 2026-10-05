//! Sorts till item names into categories for the leaderboard.
//!
//! Item names are free text from the school's tills ("traybake", "milkshake 1.05"), so each
//! category is a list of keywords matched against them. Categories come from a YAML file
//! (`CATEGORIES_FILE`), or the defaults in `categories.default.yaml`.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::Deserialize;

const DEFAULT_CATEGORIES: &str = include_str!("categories.default.yaml");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// Identifier used in entity IDs and MQTT payloads, e.g. "puddings".
    pub key: String,
    /// Display name, e.g. "Puddings".
    pub name: String,
    pub icon: Option<String>,
    keywords: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Categories(Vec<Category>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoriesFile {
    categories: IndexMap<String, CategorySpec>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CategorySpec {
    Keywords(Vec<String>),
    Full {
        keywords: Vec<String>,
        icon: Option<String>,
    },
}

impl Categories {
    /// Loads categories from `path`, or the built-in defaults when it's `None`.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        match path {
            Some(path) => {
                let yaml = std::fs::read_to_string(path)
                    .with_context(|| format!("reading categories file {}", path.display()))?;
                Self::from_yaml(&yaml)
                    .with_context(|| format!("invalid categories file {}", path.display()))
            }
            None => Self::from_yaml(DEFAULT_CATEGORIES).context("invalid default categories"),
        }
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let file: CategoriesFile = serde_yaml_ng::from_str(yaml)?;
        let mut keys = HashSet::new();
        let mut categories = Vec::new();
        for (name, spec) in file.categories {
            let (keywords, icon) = match spec {
                CategorySpec::Keywords(keywords) => (keywords, None),
                CategorySpec::Full { keywords, icon } => (keywords, icon),
            };
            let key = slug(&name);
            if key.is_empty() {
                bail!("category {name:?} needs at least one letter or digit in its name");
            }
            if !keys.insert(key.clone()) {
                bail!("two categories have the same identifier {key:?}; rename one of them");
            }
            let keywords: Vec<String> = keywords
                .iter()
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .collect();
            if keywords.is_empty() {
                bail!("category {name:?} has no keywords");
            }
            categories.push(Category {
                key,
                name: display_name(&name),
                icon,
                keywords,
            });
        }
        Ok(Self(categories))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Category> {
        self.0.iter()
    }
}

impl Category {
    pub fn matches(&self, item_name: &str) -> bool {
        let name = item_name.to_lowercase();
        self.keywords.iter().any(|k| name.contains(k.as_str()))
    }
}

/// "Hot Puddings!" -> "hot_puddings"
fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    slug.trim_matches('_').to_string()
}

/// "puddings" -> "Puddings"; names already capitalised are left alone.
fn display_name(name: &str) -> String {
    let name = name.trim();
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys_matching(categories: &Categories, item: &str) -> Vec<String> {
        categories
            .iter()
            .filter(|c| c.matches(item))
            .map(|c| c.key.clone())
            .collect()
    }

    #[test]
    fn defaults_categorise_items_seen_at_school() {
        let categories = Categories::load(None).unwrap();
        assert_eq!(keys_matching(&categories, "traybake"), ["puddings"]);
        assert_eq!(keys_matching(&categories, "Cupcake"), ["puddings"]);
        assert_eq!(
            keys_matching(&categories, "waffle/ sweet snack"),
            ["puddings"]
        );
        assert_eq!(keys_matching(&categories, "milkshake 1.05"), ["drinks"]);
        assert_eq!(keys_matching(&categories, "slushies"), ["drinks"]);
        assert_eq!(
            keys_matching(&categories, "radnor flavoured water"),
            ["drinks"]
        );
        assert!(keys_matching(&categories, "Panini").is_empty());
    }

    #[test]
    fn supports_list_and_object_forms_and_overlapping_categories() {
        let categories = Categories::from_yaml(
            "categories:\n  Pizza: [pizza]\n  Healthy veg:\n    icon: mdi:carrot\n    keywords: [veggie, Fruit]\n",
        )
        .unwrap();

        let all: Vec<_> = categories
            .iter()
            .map(|c| (c.key.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(all, [("pizza", "Pizza"), ("healthy_veg", "Healthy veg")]);
        assert_eq!(
            categories.iter().nth(1).unwrap().icon.as_deref(),
            Some("mdi:carrot")
        );
        assert_eq!(
            keys_matching(&categories, "pizza veggie"),
            ["pizza", "healthy_veg"]
        );
        assert_eq!(keys_matching(&categories, "fruit salad"), ["healthy_veg"]);
    }

    #[test]
    fn rejects_invalid_files() {
        assert!(Categories::from_yaml("categories:\n  pudding: []\n").is_err());
        assert!(Categories::from_yaml("categories:\n  '!!!': [x]\n").is_err());
        assert!(Categories::from_yaml("categories:\n  Hot Food: [a]\n  hot-food: [b]\n").is_err());
        assert!(Categories::from_yaml("pudding: [x]\n").is_err());
    }
}
