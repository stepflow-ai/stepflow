// For now, we implement expression serde via serde_json::Value. While this may
// lead to additional copies, it is significantly simpler. In the future, we can
// benchmark and implement direct serde (using the existing tests) for better
// performance if needed.
