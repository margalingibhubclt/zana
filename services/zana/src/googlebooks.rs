/*!
Queries book data from Google Books API using the [`Client`](struct@Client)
implementation of [`BookClient`](trait@BookClient).

It queries the `volumes` endpoints to retrieve data about a book, its author and ratings.

See example [here](../index.html#example).
 */
use async_trait::async_trait;
use serde::Deserialize;

use crate::{create_http_client, Book, BookClient, ClientError, Rating};

const VOLUMES_PATH: &str = "/books/v1/volumes";

#[derive(Deserialize, Debug)]
struct Volume {
    items: Option<Vec<VolumeItem>>,
}

#[derive(Deserialize, Debug)]
struct VolumeItem {
    #[serde(rename(deserialize = "volumeInfo"))]
    info: VolumeInfo,
}

#[derive(Deserialize, Debug)]
struct VolumeInfo {
    #[serde(rename(deserialize = "title"))]
    _title: String,
    #[serde(rename(deserialize = "authors"))]
    _authors: Vec<String>,
    description: String,
    #[serde(rename(deserialize = "pageCount"))]
    page_count: u32,
    #[serde(rename(deserialize = "averageRating"))]
    average_rating: f32,
    #[serde(rename(deserialize = "ratingsCount"))]
    ratings_count: u32,
}

/// Client used to retrieve data from Google Books API.
pub struct Client {
    api_key: String,
    api_url: String,
    http_client: reqwest::Client,
}

impl Client {
    /// Returns a new client that will make requests using the given API key to
    /// the given API URL.
    pub fn new(api_key: &str, api_url: &str) -> Result<Self, ClientError> {
        let http_client = create_http_client()?;
        Ok(Client {
            api_key: String::from(api_key),
            api_url: String::from(api_url),
            http_client,
        })
    }

    fn create_book(&self, items: Vec<VolumeItem>) -> Result<Book, ClientError> {
        if items.is_empty() {
            return Err(ClientError::NotFound);
        }

        let volume_item = &items[0];
        let volume_info = &volume_item.info;

        let rating = Rating::new(volume_info.average_rating, volume_info.ratings_count);
        Ok(Book::new_with_rating(
            volume_info.page_count,
            &volume_info.description,
            rating,
        ))
    }

    async fn fetch_book(&self, query: &str) -> Result<Book, ClientError> {
        let query_list: Vec<(&str, &str)> = vec![
            ("key", &self.api_key),
            ("maxResults", "1"),
            ("fields", "items"),
            ("q", query),
        ];

        let response = self
            .http_client
            .get(format!("{}{}", self.api_url, VOLUMES_PATH))
            .header("Accept-Encoding", "gzip")
            .query(&query_list)
            .send()
            .await?;

        let status_code = response.status().as_u16();
        if status_code == 429 || status_code == 403 {
            return Err(ClientError::RateLimitExceeded);
        } else if status_code < 200 || status_code >= 300 {
            let response_body = response.text().await?;
            return Err(ClientError::Http(status_code, response_body));
        }

        let volume: Volume = response.json().await?;

        if let Some(items) = volume.items {
            return self.create_book(items);
        }
        Err(ClientError::NotFound)
    }
}

#[async_trait]
impl BookClient for Client {
    /// Returns a book by ISBN.
    ///
    /// Volumes endpoint of Google Books API is queried.
    /// If an error occurs with the communication, an HTTP status code that is not 200 is returned,
    /// the book is not found, or the rate limit is exceeded then an error is returned.
    async fn book_by_isbn(&self, isbn: &str) -> Result<Book, ClientError> {
        self.fetch_book(&format!("isbn:{}", isbn)).await
    }

    /// Returns a book by author and title.
    ///
    /// Volumes endpoint of Google Books API is queried.
    /// If an error occurs with the communication, an HTTP status code that is not 200 is returned,
    /// the book is not found, or the rate limit is exceeded then an error is returned.
    async fn book(&self, author: &str, title: &str) -> Result<Book, ClientError> {
        self.fetch_book(&format!("inauthor:{} intitle:{}", author, title))
            .await
    }
}