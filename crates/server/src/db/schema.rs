diesel::table! {
    certificates (id) {
        id -> Text,
        source -> Text,
        provider -> Text,
        names -> Text,
        cert_pem -> Nullable<Text>,
        key_pem -> Nullable<Text>,
        ca_pem -> Nullable<Text>,
        chain_pem -> Nullable<Text>,
        expires_at -> BigInt,
        prefer_renew_before -> Nullable<BigInt>,
        prefer_renew_after -> Nullable<BigInt>,
        requested_at -> BigInt,
        created_at -> BigInt,
        updated_at -> BigInt,
    }
}
