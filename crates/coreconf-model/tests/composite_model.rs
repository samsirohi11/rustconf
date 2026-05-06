use coreconf_model::CompositeModel;

#[test]
fn composite_model_resolves_multiple_sid_files() {
    let model = CompositeModel::from_sid_strings(&[
        r#"{"module-name":"example-a","module-revision":"2026-01-01","item":[
            {"identifier":"example-a","sid":60000},
            {"identifier":"/example-a:root","sid":60001}
        ],"key-mapping":{}}"#,
        r#"{"module-name":"example-b","module-revision":"2026-01-01","item":[
            {"identifier":"example-b","sid":61000},
            {"identifier":"/example-b:leaf","sid":61001,"type":"string"}
        ],"key-mapping":{}}"#,
    ])
    .unwrap();

    assert_eq!(model.get_sid("/example-a:root"), Some(60001));
    assert_eq!(model.get_sid("/example-b:leaf"), Some(61001));
}
