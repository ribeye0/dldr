use super::*;
use std::sync::atomic::AtomicUsize;

struct CreditSource {
    calls: AtomicUsize,
    fail: bool,
    works: Vec<PublicWork>,
}

#[async_trait]
impl LocalWorkMetadataSource for CreditSource {
    async fn works(&self, _: &[WorkId]) -> Result<Vec<PublicWork>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        tokio::task::yield_now().await;
        if self.fail {
            return Err(LibraryError::SyncSource("offline".into()));
        }
        Ok(self.works.clone())
    }
}

async fn missing_voice_work() -> Result<(Library, AccountSyncCommit)> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;
    let cached = cached_work_from_api(
        work(
            "RJ01729125",
            "Purchased title",
            "Purchased circle",
            "2026-01-01T00:00:00Z",
        ),
        "2026-01-01",
    )?;
    let sync = AccountSyncCommit {
        sync_run_id: "initial".into(),
        account_id: "account-a".into(),
        started_at: "2026-01-01".into(),
        completed_at: "2026-01-01".into(),
        works: vec![cached],
        account_works: vec![AccountWork {
            work_id: "RJ01729125".into(),
            purchased_at: None,
        }],
    };
    library.storage.commit_account_sync(&sync).await?;
    Ok((library, sync))
}

fn two_voices() -> PublicWork {
    serde_json::from_value(json!({
        "workno":"RJ01729125", "work_name":"Public title must not replace purchased names",
        "creaters": {"voice_by": [
            {"id":"64570", "name":"伊倉える", "classification":"voice_by"},
            {"id":"17542", "name":"浅木式", "classification":"voice_by"}
        ]}, "voice_by":null, "tags":null,
        "genres": [{"id":500, "name":"耳舐め"}]
    }))
    .unwrap()
}

#[tokio::test]
async fn empty_play_tags_are_supplemented_with_both_public_actors_and_survive_sync() -> Result<()> {
    let (library, mut sync) = missing_voice_work().await?;
    let before = library.product_detail("RJ01729125").await?;
    assert!(before.credit_groups.is_empty());
    let source = CreditSource {
        calls: AtomicUsize::new(0),
        fail: false,
        works: vec![two_voices()],
    };
    let (a, b) = tokio::join!(
        library.product_detail_with_metadata_source("RJ01729125", &source),
        library.product_detail_with_metadata_source("RJ01729125", &source)
    );
    let a = a?;
    assert_eq!(a, b?);
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    assert_eq!(a.credit_groups[0].names, vec!["伊倉える", "浅木式"]);
    assert_eq!(a.title, before.title);
    assert_eq!(a.maker_names, before.maker_names);
    assert_eq!(a.content_size_bytes, before.content_size_bytes);
    assert_eq!(a.download, before.download);
    assert_eq!(a.owners, before.owners);
    assert_eq!(
        a.tags
            .iter()
            .filter(|tag| tag.class == "genre")
            .collect::<Vec<_>>(),
        before
            .tags
            .iter()
            .filter(|tag| tag.class == "genre")
            .collect::<Vec<_>>()
    ); // Public genres never overwrite owned classifications.
    sync.sync_run_id = "next".into();
    library.storage.commit_account_sync(&sync).await?;
    assert_eq!(
        library.product_detail("RJ01729125").await?.credit_groups,
        a.credit_groups
    );
    for name in ["伊倉える", "浅木式"] {
        let page = library
            .list_products(&ProductListQuery {
                search: Some(name.into()),
                ..Default::default()
            })
            .await?;
        assert_eq!(page.total_count, 1);
        assert_eq!(page.products[0].credit_groups, a.credit_groups);
    }
    Ok(())
}

#[tokio::test]
async fn successful_empty_lookup_is_cached_but_network_failure_remains_retryable() -> Result<()> {
    let (library, _) = missing_voice_work().await?;
    let failed = CreditSource {
        calls: AtomicUsize::new(0),
        fail: true,
        works: vec![],
    };
    assert!(library
        .product_detail_with_metadata_source("RJ01729125", &failed)
        .await?
        .credit_groups
        .is_empty());
    assert_eq!(
        library
            .storage
            .product_credits_checked_at("RJ01729125")
            .await?,
        None
    );
    let empty = CreditSource {
        calls: AtomicUsize::new(0),
        fail: false,
        works: vec![],
    };
    library
        .product_detail_with_metadata_source("RJ01729125", &empty)
        .await?;
    library
        .product_detail_with_metadata_source("RJ01729125", &empty)
        .await?;
    assert_eq!(empty.calls.load(Ordering::SeqCst), 1);
    library
        .storage
        .cache_public_product_credits("RJ01729125", &[], "2000-01-01T00:00:00Z")
        .await?;
    let valid = CreditSource {
        calls: AtomicUsize::new(0),
        fail: false,
        works: vec![two_voices()],
    };
    assert_eq!(
        library
            .product_detail_with_metadata_source("RJ01729125", &valid)
            .await?
            .credit_groups[0]
            .names
            .len(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn provided_production_credits_are_supplemented_even_when_voice_is_cached() -> Result<()> {
    let (library, mut sync) = missing_voice_work().await?;
    let mut raw: Value = serde_json::from_str(&sync.works[0].raw_json)?;
    raw["tags"] = json!([{"class":"voice_by","name":{"ja_JP":"Cached actor"}}]);
    sync.works[0].raw_json = serde_json::to_string(&raw)?;
    sync.sync_run_id = "has-voice".into();
    library.storage.commit_account_sync(&sync).await?;
    let source = CreditSource {
        calls: AtomicUsize::new(0),
        fail: false,
        works: vec![serde_json::from_value(json!({
            "workno":"RJ01729125", "series_name":"シリーズ名",
            "creaters":{"created_by":[{"name":"作者"}],"illust_by":[{"name":"絵師"}]}
        }))?],
    };
    let detail = library
        .product_detail_with_metadata_source("RJ01729125", &source)
        .await?;
    assert_eq!(source.calls.load(Ordering::SeqCst), 1);
    for (kind, name) in [
        ("series", "シリーズ名"),
        ("creator", "作者"),
        ("illust", "絵師"),
        ("voice", "Cached actor"),
    ] {
        assert!(detail
            .credit_groups
            .iter()
            .any(|group| group.kind == kind && group.names.iter().any(|n| n == name)));
    }
    assert!(!detail
        .credit_groups
        .iter()
        .any(|group| group.kind == "scenario"));
    Ok(())
}
