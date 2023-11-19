Derive schema function return tantivy::schema::Schema;
And impl into tantivy::schema::Document.
```
#[derive(Schema)]
pub struct Doc {
    #[field(name = "str", stored, indexed)]
    text: String,
    #[field(fast, norm, coerce, indexed)]
    num: u64,
    #[field(stored, indexed, fast)]
    date: DateTime,
    #[field(stored, indexed)]
    facet: Facet,
    #[field(stored, indexed)]
    bytes: Vec<u8>,
    #[field(stored, indexed)]
    json: Map<String, Value>,
    #[field(fast, indexed)]
    ip: Ipv6Addr
}
```