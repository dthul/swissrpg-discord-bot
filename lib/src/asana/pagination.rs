use super::api::{AsyncClient, Error};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct NextPage {
    offset: String,
    // path: String,
    // uri: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PaginatedApiResponse<T> {
    data: Vec<T>,
    next_page: Option<NextPage>,
}

impl AsyncClient {
    pub(super) async fn get_all<T: for<'de> Deserialize<'de>>(
        &self,
        url: &'_ str,
    ) -> Result<Vec<T>, Error> {
        // No offset query parameter for the first page
        let mut offset = None;
        let mut results = vec![];
        loop {
            let mut req = self.client.get(url).query(&[("limit", "100")]);
            if let Some(offset) = offset {
                req = req.query(&[("offset", offset)]);
            }
            let res = req.send().await?;
            let res: PaginatedApiResponse<T> = Self::try_deserialize(res).await?;
            results.extend(res.data.into_iter());
            if let Some(next_page) = res.next_page {
                // Store the offset for the next page and loop
                offset = Some(next_page.offset);
            } else {
                // No next page
                break;
            }
        }
        Ok(results)
    }
}
