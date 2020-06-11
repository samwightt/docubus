use serde_json::{from_str, Value};
use valico::json_schema;
use valico::common::error::ValicoErrors;
use crate::cache;
use anyhow::{Result, Context, anyhow};
use tokio::prelude::*;
use tokio::fs::File;

/// An internal representation of the Schema. Contains useful functions for 
/// loading the schema into the cache, downloading the schema if necessary,
/// and validating the schema against the JSON.
///
/// The schema caches the Serde Value of itself in the `val` variable. By default,
/// `val` is set to `None`. `val` is only populated if the schema is loaded from the filesystem,
/// either using `load()` or `load_or_download()`.
#[derive(Clone)]
pub struct Schema {
    val: Option<Value>,
}

impl Schema {
    /// Creates a new schema, where `path` is the path to the schema (either existing
    /// or where one should be stored if it is downloaded). Sets the cached value to empty by default.
    pub async fn new() -> Result<Self> {
        Ok(Schema {
            val: None,
        })
    }

    /// Attempts to load the schema from the file system, returning a copy of it.
    /// Caches the schema for later use.
    pub async fn load(&mut self) -> Result<Value> {
        if let Some(schema) = self.val.clone() {
            Ok(schema)
        }
        else {
            let file_path = cache::get_path("schema.json").await?;
            let mut file = File::open(file_path).await.context("Could not open schema.json")?;
            let mut buffer = String::new();
            file.read_to_string(&mut buffer).await?;
            let schema: Value = from_str(&buffer).context("Failed to parse schema.min.json.")?;
            self.val = Some(schema.clone());
            Ok(schema)
        }
    }

    /// Downloads the schema from the GitHub source.
    /// Fails if the schema isn't accessible. Doesn't continue if the file already exists.
    pub async fn download(&mut self) -> Result<()> {
        println!("Could not find schema.json in cache folder, downloading now...");
        let schema_path = cache::get_path("schema.json").await.context("Could not get path to schema.")?;
        if !schema_path.exists() {
            let result = reqwest::get("https://raw.githubusercontent.com/samwightt/ibis/master/schema.min.json").await
                .context("Could not get schema.min.json from GitHub source.")?
                .text().await
                .context("Could not get schema.min.json from GitHub source.")?;
            let mut out = cache::create_file("schema.json").await.context("Could not create schema.min.json.")?;
            out.write_all(&result.as_bytes()).await.context("Could not write schema.min.json.")?;
        }
        else {
            return Err(anyhow!("Tried to download schema.min.json but it already existed."));
        }
        println!("Downloaded schema.min.json to cache folder.");

        Ok(())
    }

    /// Gets the cached version of the schema. Returns Some(schema) if it exists,
    /// or returns None if it does not.
    pub fn get(&self) -> Option<Value> {
        self.val.clone()
    }

    /// Tries to load the schema from the filesystem. If the schema does not exist,
    /// it downloads the schema from the GitHub source, then loads it from the filesystem.
    pub async fn load_or_download(&mut self) -> Result<Value> {
        if let Ok(schema) = self.load().await {
            Ok(schema)
        }
        else {
            self.download().await.context("Could not download schema.")?;
            Ok(self.load().await.context("Could not load schema from filesystem.")?)
        }
    }

    /// Validates a Serde Value against the Schema to be sure it fits all the required specs.
    /// Returns None if there are no errors, and returns Some(errors) if the JSON isn't valid against the schema.
    /// Calls `load_or_download()` to get the schema.
    /// *Beware the gnarly Valico errors!*
    pub async fn validate(&mut self, val: &Value) -> Result<Option<ValicoErrors>> {
        let schema_json = self.load_or_download().await?;
        let mut scope: json_schema::Scope = json_schema::Scope::new();

        let schema = scope
            .compile_and_return(schema_json, false).unwrap();
        
        let validate = schema.validate(&val);
        if !validate.is_valid() {
            Ok(Some(validate.errors))
        }
        else {
            Ok(None)
        }
    }

    /// Gets a JSON file, converting it to a Serde Value, then validates it against the schema.
    /// Returns None if there are no errors, and returns Some(errors) if the JSON isn't valid against the schema.
    /// Calls `load_or_download()` to get the schema.
    /// *Beware the gnarly Valico errors!*
    pub async fn validate_file(&mut self, file: &str) -> Result<Option<ValicoErrors>> {
        let mut file = File::open(&file).await.context("Could not open file to verify.")?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer).await?;
        let to_validate: Value = from_str(&buffer).context("Could not open file to parse.")?;
        self.validate(&to_validate).await
    }
}