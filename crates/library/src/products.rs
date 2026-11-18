use super::*;

impl Library {
    pub(super) async fn resolved_query(
        &self,
        query: &ProductListQuery,
    ) -> Result<ProductListQuery> {
        let mut query = query.clone();
        query.language_priority = self.storage.app_settings().await?.language_priority;
        Ok(query)
    }

    pub async fn list_products(&self, query: &ProductListQuery) -> Result<ProductListPage> {
        Ok(self
            .storage
            .list_products(&self.resolved_query(query).await?)
            .await?)
    }

    pub async fn available_name_languages(&self) -> Result<Vec<String>> {
        Ok(self.storage.available_name_languages().await?)
    }

    pub async fn product_filter_facets(
        &self,
        query: &ProductListQuery,
    ) -> Result<ProductFilterFacets> {
        Ok(self
            .storage
            .product_filter_facets(&self.resolved_query(query).await?)
            .await?)
    }

    pub async fn product_detail(&self, work_id: &str) -> Result<ProductDetail> {
        let priority = self.storage.app_settings().await?.language_priority;
        Ok(self.storage.product_detail(work_id, &priority).await?)
    }
}
