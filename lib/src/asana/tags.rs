use super::api::*;
use serde::{Deserialize, Serialize, Serializer};
use serde_json::json;

#[derive(Debug, Deserialize, Clone)]
pub struct Tag {
    #[serde(rename = "gid")]
    pub id: TagId,
    pub name: String,
    pub color: Optional<Color>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateTag {
    pub name: String,
    pub color: Option<Color>,
}

#[derive(Debug, Deserialize, Clone)]
struct Tags {
    data: Vec<Tag>,
}

#[derive(Debug, Clone)]
pub enum Color {
    DarkBlue,
    DarkBrown,
    DarkGreen,
    DarkOrange,
    DarkPink,
    DarkPurple,
    DarkRed,
    DarkTeal,
    DarkWarmGray,
    LightBlue,
    LightGreen,
    LightOrange,
    LightPink,
    LightPurple,
    LightRed,
    LightTeal,
    LightWarmGray,
    LightYellow,
    // Just in case Asana adds new colors:
    Other(String),
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "dark-blue" => Ok(Color::DarkBlue),
            "dark-brown" => Ok(Color::DarkBrown),
            "dark-green" => Ok(Color::DarkGreen),
            "dark-orange" => Ok(Color::DarkOrange),
            "dark-pink" => Ok(Color::DarkPink),
            "dark-purple" => Ok(Color::DarkPurple),
            "dark-red" => Ok(Color::DarkRed),
            "dark-teal" => Ok(Color::DarkTeal),
            "dark-warm-gray" => Ok(Color::DarkWarmGray),
            "light-blue" => Ok(Color::LightBlue),
            "light-green" => Ok(Color::LightGreen),
            "light-orange" => Ok(Color::LightOrange),
            "light-pink" => Ok(Color::LightPink),
            "light-purple" => Ok(Color::LightPurple),
            "light-red" => Ok(Color::LightRed),
            "light-teal" => Ok(Color::LightTeal),
            "light-warm-gray" => Ok(Color::LightWarmGray),
            "light-yellow" => Ok(Color::LightYellow),
            _ => Ok(Color::Other(s)),
        }
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Color::DarkBlue => serializer.serialize_str("dark-blue"),
            Color::DarkBrown => serializer.serialize_str("dark-brown"),
            Color::DarkGreen => serializer.serialize_str("dark-green"),
            Color::DarkOrange => serializer.serialize_str("dark-orange"),
            Color::DarkPink => serializer.serialize_str("dark-pink"),
            Color::DarkPurple => serializer.serialize_str("dark-purple"),
            Color::DarkRed => serializer.serialize_str("dark-red"),
            Color::DarkTeal => serializer.serialize_str("dark-teal"),
            Color::DarkWarmGray => serializer.serialize_str("dark-warm-gray"),
            Color::LightBlue => serializer.serialize_str("light-blue"),
            Color::LightGreen => serializer.serialize_str("light-green"),
            Color::LightOrange => serializer.serialize_str("light-orange"),
            Color::LightPink => serializer.serialize_str("light-pink"),
            Color::LightPurple => serializer.serialize_str("light-purple"),
            Color::LightRed => serializer.serialize_str("light-red"),
            Color::LightTeal => serializer.serialize_str("light-teal"),
            Color::LightWarmGray => serializer.serialize_str("light-warm-gray"),
            Color::LightYellow => serializer.serialize_str("light-yellow"),
            Color::Other(s) => serializer.serialize_str(s),
        }
    }
}

impl AsyncClient {
    // Searches for a tag by name. If a tag with this name does not exist, it
    // is created. The supplied color is only used when a tag is newly created,
    // an existing tag with the specified name will not have its color changed.
    // If there exist several tags with the same name, this functions returns
    // an arbitrary one of them.
    pub async fn get_or_create_tag_by_name(
        &self,
        name: &str,
        color: Option<&Color>,
    ) -> Result<Tag, Error> {
        // Check the cache first
        {
            let tags = self.tags.read().await;
            if let Some(tag) = tags.values().find(|tag| tag.name == name) {
                return Ok(tag.clone());
            }
        }
        // No hit in the cache, get all tags from the Asana API
        let url = format!(
            "{}/workspaces/{}/tags?opt_fields=name,color",
            BASE_URL, self.workspace.0
        );
        let all_tags = self.get_all(&url).await?;
        let tag = {
            // Update the cache with all tags
            let mut tags = self.tags.write().await;
            all_tags.into_iter().for_each(|tag: Tag| {
                tags.insert(tag.id.clone(), tag);
            });
            // Now check the cache again
            if let Some(tag) = tags.values().find(|tag| tag.name == name) {
                return Ok(tag.clone());
            }
            // Still no hit? Then this tag doesn't exist.
            // We create it now and hold the write lock on the tag cache while
            // doing so, to prevent a tag from being created multiple times.
            let url = format!("{}/tags", BASE_URL);
            let new_tag = CreateTag {
                name: name.to_string(),
                color: color.cloned(),
            };
            let payload = json!({ "data": new_tag });
            let res = self
                .client
                .post(&url)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(payload.to_string())
                .send()
                .await?;
            let tag: Wrapper<Tag> = Self::try_deserialize(res).await?;
            let tag = tag.data;
            // Store the newly created tag in the cache
            tags.insert(tag.id.clone(), tag.clone());
            tag
        };
        Ok(tag)
    }
}
