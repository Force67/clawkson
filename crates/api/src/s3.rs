use std::sync::Arc;

use aws_sdk_s3::{
    config::{Credentials, Region},
    primitives::ByteStream,
    Client,
};

pub struct S3Storage {
    client: Client,
    bucket: String,
}

impl S3Storage {
    /// Try to connect to S3-compatible storage. Returns None if env vars are missing.
    pub async fn try_connect() -> Option<Arc<S3Storage>> {
        let endpoint = std::env::var("CLAWKSON_S3_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:9100".to_string());
        let access_key = std::env::var("CLAWKSON_S3_ACCESS_KEY")
            .unwrap_or_else(|_| "clawkson".to_string());
        let secret_key = std::env::var("CLAWKSON_S3_SECRET_KEY")
            .unwrap_or_else(|_| "clawkson-secret-key".to_string());
        let bucket = std::env::var("CLAWKSON_S3_BUCKET")
            .unwrap_or_else(|_| "clawkson-documents".to_string());

        let creds = Credentials::new(&access_key, &secret_key, None, None, "env");

        let config = aws_sdk_s3::Config::builder()
            .endpoint_url(&endpoint)
            .region(Region::new("us-east-1"))
            .credentials_provider(creds)
            .force_path_style(true)
            .behavior_version_latest()
            .build();

        let client = Client::from_conf(config);

        let storage = S3Storage { client, bucket };

        if let Err(e) = storage.create_bucket_if_not_exists().await {
            tracing::warn!(endpoint = %endpoint, error = %e, "cannot reach S3 storage — is RustFS running?");
            return None;
        }

        tracing::info!(endpoint = %endpoint, bucket = %storage.bucket, "S3 storage ready");
        Some(Arc::new(storage))
    }

    async fn create_bucket_if_not_exists(&self) -> anyhow::Result<()> {
        match self.client.head_bucket().bucket(&self.bucket).send().await {
            Ok(_) => return Ok(()),
            Err(e) => {
                // Bucket doesn't exist or not accessible — try to create
                tracing::debug!("Bucket head check failed, attempting create: {e}");
            }
        }

        self.client
            .create_bucket()
            .bucket(&self.bucket)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create bucket '{}': {e}", self.bucket))?;

        Ok(())
    }

    pub async fn put_object(&self, key: &str, data: Vec<u8>, content_type: &str) -> anyhow::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(ByteStream::from(data))
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 put_object failed: {e}"))?;
        Ok(())
    }

    pub async fn get_object(&self, key: &str) -> anyhow::Result<(Vec<u8>, String)> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 get_object failed: {e}"))?;

        let content_type = resp
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| anyhow::anyhow!("S3 body read failed: {e}"))?
            .into_bytes()
            .to_vec();

        Ok((bytes, content_type))
    }

    pub async fn delete_object(&self, key: &str) -> anyhow::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("S3 delete_object failed: {e}"))?;
        Ok(())
    }
}
