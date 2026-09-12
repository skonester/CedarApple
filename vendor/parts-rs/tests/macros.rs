#[cfg(test)]
mod tests {
    use parts::value::{
        FromPartsObject, FromValue, IntoPartsObject, IntoValue, Value, parts_native,
    };

    #[derive(FromPartsObject, IntoPartsObject, Debug, PartialEq, Clone)]
    struct TestMediaItem {
        id: String,
        duration: i64,
        is_playable: bool,
        season_index: Option<i32>,
    }

    #[test]
    fn test_parts_object_roundtrip() {
        let original = TestMediaItem {
            id: "anime_123".to_string(),
            duration: 1400,
            is_playable: true,
            season_index: Some(2),
        };

        let parts_val = original.clone().into_value();

        assert!(matches!(parts_val, Value::Object(_)));

        let parsed = TestMediaItem::from_value(&parts_val).expect("Failed to parse struct");

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_parts_object_missing_fields() {
        let mut map = rustc_hash::FxHashMap::default();

        let mut insert_field = |key: &str, val: Value| {
            map.insert(Value::String(key.to_string().into()).get_hash(), val);
        };

        insert_field("id", Value::String("movie_456".to_string().into()));
        insert_field("duration", Value::Int(5000));
        insert_field("is_playable", Value::Bool(false));

        let parts_val = Value::Object(std::rc::Rc::new(std::cell::RefCell::new(map)));

        let parsed = TestMediaItem::from_value(&parts_val).expect("Failed to parse partial object");

        assert_eq!(parsed.id, "movie_456");
        assert_eq!(parsed.duration, 5000);
        assert!(!parsed.is_playable);
        assert_eq!(parsed.season_index, None)
    }

    #[test]
    fn test_parts_vec_translation() {
        let items = vec!["First".to_string(), "Second".to_string()];

        let parts_val = items.clone().into_value();

        // Convert back
        let parsed_items = Vec::<String>::from_value(&parts_val).expect("Failed to parse Vec");

        assert_eq!(items, parsed_items);
    }

    #[parts_native]
    fn multiply_by_two(val: i64) -> Result<i64, String> {
        Ok(val * 2)
    }

    #[test]
    fn test_parts_native_macro() {
        let args = vec![Value::Int(21)];
        let res = multiply_by_two(&args).unwrap();
        assert_eq!(res, Value::Int(42));
    }
}
