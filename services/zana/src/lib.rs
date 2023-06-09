/*!
This crate provides functionality to retrieve data from the following
book related APIs:
- Client for [OpenLibrary](https://openlibrary.org/)
- Client for [Google Books](https://developers.google.com/books)

Data is retrieved through calls being made by implementations of [`BookClient`](trait@BookClient).

## Client for Google Books API

When querying from Google Books API, one API calls is made to the _volumes_ endpoint,
to retrieve data by ISBN of a book. In cases where no data is found by ISBN,
then book title and author are used as a backup.
[`Client`](struct@googlebooks::Client) is used to query data from Google Books API.

### Example

```
use zana::{Book, BookClient, ClientError};
use zana::googlebooks::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_url = "https://www.googleapis.com";
    let api_key = "YOUR-API-KEY";
    let isbn = "9780316387316";

    let client = Client::new(api_key, api_url)?;

    match client.book_by_isbn(isbn).await {
        Ok(book) => println!("book found ({}: {:?})", isbn, &book),
        Err(err) => eprintln!("could not fetch book by ISBN {:?}", err),
    };
    Ok(())
}
```

## Client for OpenLibrary

[`Client`](struct@openlibrary::Client) for OpenLibrary makes three separate API calls:
1. Fetch book by ISBN
2. Fetch `work` of the book (A _work_ here being a logical collection of similar editions)
3. Fetch ratings

### Example

```
use zana::{Book, BookClient, ClientError};
use zana::openlibrary::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_url = "https://openlibrary.org";
    let isbn = "9780316387316";

    let client = Client::new(api_url)?;

    match client.book_by_isbn(isbn).await {
        Ok(book) => println!("book found ({}: {:?})", isbn, &book),
        Err(err) => eprintln!("could not fetch book by ISBN {:?}", err),
    };
    Ok(())
}
```

## Returned data

For both implementations, all the data is grouped into the [Book](struct@Book) type which
is returned from clients.

For status codes that are not 200, [ClientError](enum@ClientError) is returned with more
information about the source of the error.
*/

extern crate core;

use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

pub mod googlebooks;
pub mod openlibrary;

/// An error that occurs for implementations of [BookClient][trait@BookClient].
///
/// The error will contain different variants to make handling of errors easier.
/// Some specific Http status codes (e.g. `429` _Too Many Requests_ or `404` _Not Found_) will have
/// their own variant because of their importance or the custom handling they will require.
#[derive(Error, Debug)]
pub enum ClientError {
    /// Occurs for any error that comes from [reqwest](reqwest) crate. This will include errors
    /// when a timeout is reached, or no connection can be made to the specified endpoint.
    #[error("error coming from internal http client")]
    InternalClient(#[from] reqwest::Error),
    /// Occurs when a 429 or (403 in some clients) status code is returned from the response.
    #[error("rate limit exceeded for external service")]
    RateLimitExceeded,
    /// Occurs when queried book is not found
    #[error("book is not found")]
    NotFound,
    /// Occurs for any response that is not 200, 404 or 429 (403 included for some clients).
    #[error("generic http error that contains status code and response body")]
    Http(u16, String),
}

/// Book data retrieved from third-party services supported by the crate.
///
/// Data retrieved by clients, in some cases by multiple API calls, will be aggregated
/// and returned as this type.
///
/// [page_count](struct@Book.page_count) is returned as 0 if its not provided
/// by the third-party service.
///
/// [description](struct@Book.description) is returned as an empty string if its not
/// provided by the third-party service.
///
/// [rating](struct@Book.rating) is optional, since in some cases books either may not have
/// rating data available yet, or other third-party services that can be added in the future
/// may not provide ratings at all.
#[derive(Debug, PartialEq)]
pub struct Book {
    /// Number of pages, 0 if not provided by the third-party service
    pub page_count: u32,
    /// Book description, empty if not provided by the third-party service
    pub description: String,
    /// Link to view the book at the third-party service
    pub provider_link: String,
    pub rating: Option<Rating>,
}

/// Rating data retrieved from third-party services.
///
/// This data holds only the average rating as a floating point, and the number of
/// ratings given.
#[derive(Debug, PartialEq)]
pub struct Rating {
    pub average_rating: f32,
    pub ratings_count: u32,
}

impl Book {
    /// Returns a Book with defaults for optional data.
    ///
    /// - rating is optional, and by default is [`None`](None)
    pub fn new(page_count: u32, description: &str, provider_link: &str) -> Self {
        Self {
            page_count,
            description: String::from(description),
            provider_link: String::from(provider_link),
            rating: None,
        }
    }

    /// Returns a Book with required data and ratings
    pub fn new_with_rating(
        page_count: u32,
        description: &str,
        provider_link: &str,
        rating: Rating,
    ) -> Self {
        let mut book = Book::new(page_count, description, provider_link);
        book.rating = Some(rating);
        book
    }
}

impl Rating {
    /// Returns a new rating.
    ///
    /// Meant only to be created for ratings that are valid and exist.
    /// In this case a rating that is '_valid_' is one that is `null` or does not
    /// have a `ratings_count` *or* `average_rating` of 0 when retrieved from the third-party service.
    pub fn new(average_rating: f32, ratings_count: u32) -> Self {
        Self {
            average_rating,
            ratings_count,
        }
    }
}

/// A trait that describes implementations of API clients for third-party API services.
///
/// This trait provides a way to access different APIs and returns the data in a standard format.
/// Different APIs may require multiple requests, or requests that
/// are differently configured to retrieve the data.
/// This trait provides different ways of which the data can be retrieved.
///
/// In cases where a third-party API does not support one of the ways to retrieve data,
/// then `unimplemented!` is used, to indicate that
/// a [Book](struct@Book) cannot not be queried using that functionality.
///
/// When there's an error with communication/network, and the request cannot be completed,
/// the rate limit has been reached, the book could not be found,
/// or a HTTP status code has been returned that is not 200, then an error will be returned.
#[async_trait]
pub trait BookClient {
    /// Returns a book from the given ISBN.
    async fn book_by_isbn(&self, isbn: &str) -> Result<Book, ClientError>;

    /// Returns a book from author and title
    async fn book(&self, author: &str, title: &str) -> Result<Book, ClientError>;
}

fn create_http_client() -> Result<reqwest::Client, reqwest::Error> {
    let version: &str = option_env!("CARGO_PKG_VERSION").unwrap_or("1.0.0");

    reqwest::Client::builder()
        .gzip(true)
        .user_agent(format!("zana/{} (gzip)", version))
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(30))
        .build()
}
