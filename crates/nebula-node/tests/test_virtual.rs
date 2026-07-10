use nebula_common::{PlacementAssignment, PlacementPlan};

#[tokio::test]
async fn test_virtual_placement_plan_shape() {
    let plan = PlacementPlan {
        request_id: None,
        model_uid: "virtual-test-model".to_string(),
        model_name: "test-model".to_string(),
        version: 1,
        leader_epoch: 1,
        assignments: vec![PlacementAssignment {
            node_id: "local-node".to_string(),
            replica_id: 0,
            port: 8080,
            engine_config_path: String::new(),
            gpu_index: None,
            gpu_indices: None,
            extra_args: None,
            engine_type: Some("virtual".to_string()),
            docker_image: None,
        }],
    };
    assert_eq!(plan.assignments[0].engine_type.as_deref(), Some("virtual"));
    assert_eq!(plan.leader_epoch, 1);
}
