#[cfg(test)]
mod tests {
    use super::*;
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
            assignments: vec![],
        };
        let val3 = serde_json::to_vec(&updated_plan).unwrap();

        let result = store.compare_and_swap(&placement_key, revision, val3).await;

        // 4. Assertion: CAS must fail
        assert!(result.is_ok());
        let (success, _) = result.unwrap();
        assert!(!success, "CAS should fail because revision 1 is stale");
    }
}
