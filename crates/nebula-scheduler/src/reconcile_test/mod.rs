#[cfg(test)]
mod tests {
    use nebula_common::{PlacementAssignment, PlacementPlan};
    use nebula_meta::{MemoryMetaStore, MetaStore};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_reconcile_cas_conflict() {
        let store = Arc::new(MemoryMetaStore::new());
        let model_uid = "test-model".to_string();
        let placement_key = format!("/placements/{}", model_uid);

        // 1. Initial State: Revision 1
        let initial_plan = PlacementPlan {
            request_id: Some("req1".into()),
            model_uid: model_uid.clone(),
            model_name: "test-model".into(),
            version: 1000,
            updated_at_ms: 1_000,
            leader_epoch: 1,
            assignments: vec![],
        };
        let val = serde_json::to_vec(&initial_plan).unwrap();
        store.put(&placement_key, val, None).await.unwrap();

        // Simulate reading: we get Revision 1
        let (data, revision) = store.get(&placement_key).await.unwrap().unwrap();
        let _plan: PlacementPlan = serde_json::from_slice(&data).unwrap();
        assert_eq!(revision, 1);

        // 2. Simulate concurrent update (Another scheduler updates to Revision 2)
        let concurrent_plan = PlacementPlan {
            request_id: Some("req2".into()),
            model_uid: model_uid.clone(),
            model_name: "test-model".into(),
            version: 2000,
            updated_at_ms: 2_000,
            leader_epoch: 1,
            assignments: vec![PlacementAssignment {
                replica_id: 1,
                node_id: "node1".into(),
                engine_config_path: "/tmp/nebula/test.yaml".into(),
                port: 8000,
                gpu_index: None,
                gpu_indices: None,
                extra_args: None,
                engine_type: None,
                docker_image: None,
                pool_id: None,
            }],
        };
        let val2 = serde_json::to_vec(&concurrent_plan).unwrap();
        store.put(&placement_key, val2, None).await.unwrap();

        // Confirm new revision is 2
        let (_, new_revision) = store.get(&placement_key).await.unwrap().unwrap();
        assert_eq!(new_revision, 2);

        // 3. Thread A (our reconciler) attempts to CAS with Revision 1
        let updated_plan = PlacementPlan {
            request_id: Some("req1".into()),
            model_uid: model_uid.clone(),
            model_name: "test-model".into(),
            version: 1001,
            updated_at_ms: 1_001,
            leader_epoch: 1,
            assignments: vec![],
        };
        let val3 = serde_json::to_vec(&updated_plan).unwrap();

        let result = store.compare_and_swap(&placement_key, revision, val3).await;

        // 4. Assertion: CAS must fail
        assert!(result.is_ok());
        let (success, _) = result.unwrap();
        assert!(!success, "CAS should fail because revision 1 is stale");
    }

    #[tokio::test]
    async fn test_leader_epoch_fencing_record() {
        // Stale leader_epoch must not overwrite a newer plan via CAS when revision moved.
        let store = Arc::new(MemoryMetaStore::new());
        let key = "/placements/m1".to_string();

        let new_leader_plan = PlacementPlan {
            request_id: None,
            model_uid: "m1".into(),
            model_name: "m".into(),
            version: 2000,
            updated_at_ms: 2_000,
            leader_epoch: 2,
            assignments: vec![],
        };
        store
            .put(&key, serde_json::to_vec(&new_leader_plan).unwrap(), None)
            .await
            .unwrap();
        let (_, rev) = store.get(&key).await.unwrap().unwrap();

        let stale = PlacementPlan {
            request_id: None,
            model_uid: "m1".into(),
            model_name: "m".into(),
            version: 1999,
            updated_at_ms: 1_999,
            leader_epoch: 1,
            assignments: vec![],
        };
        // Old leader still holds an older revision snapshot (rev-1) — CAS must fail.
        let (ok, _) = store
            .compare_and_swap(&key, rev.saturating_sub(1), serde_json::to_vec(&stale).unwrap())
            .await
            .unwrap();
        assert!(!ok);

        let (bytes, _) = store.get(&key).await.unwrap().unwrap();
        let kept: PlacementPlan = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(kept.leader_epoch, 2);
    }

    #[tokio::test]
    async fn test_logical_version_bump_on_cas_write() {
        use nebula_common::next_placement_version;

        let store = Arc::new(MemoryMetaStore::new());
        let key = "/placements/m-ver".to_string();
        let mut plan = PlacementPlan {
            request_id: None,
            model_uid: "m-ver".into(),
            model_name: "m".into(),
            version: 0,
            updated_at_ms: 0,
            leader_epoch: 1,
            assignments: vec![],
        };
        store
            .put(&key, serde_json::to_vec(&plan).unwrap(), None)
            .await
            .unwrap();

        for expected in 1u64..=3 {
            let (data, rev) = store.get(&key).await.unwrap().unwrap();
            let prev: PlacementPlan = serde_json::from_slice(&data).unwrap();
            plan.version = next_placement_version(prev.version);
            plan.updated_at_ms = expected * 1_000;
            let (ok, _) = store
                .compare_and_swap(&key, rev, serde_json::to_vec(&plan).unwrap())
                .await
                .unwrap();
            assert!(ok);
            let (bytes, _) = store.get(&key).await.unwrap().unwrap();
            let got: PlacementPlan = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(got.version, expected);
        }
    }
}
