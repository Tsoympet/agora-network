//! Strict public RPC currency parsing (no locale / lowercase normalization).

use agora_types::{Address, DrcBookAsset, DrcOfferBook, IssuedAssetId, IssuedCurrencyCode};

use crate::error::RpcError;

pub fn parse_issued_currency_code(
    value: &serde_json::Value,
) -> Result<IssuedCurrencyCode, RpcError> {
    let s = value
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams("currency must be a string".into()))?;
    if s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        let mut bytes = [0u8; 20];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            if chunk.len() != 2 {
                return Err(RpcError::InvalidParams(
                    "currency hex must be exactly 40 hex digits".into(),
                ));
            }
            let pair = std::str::from_utf8(chunk)
                .map_err(|_| RpcError::InvalidParams("currency hex must be ASCII".into()))?;
            bytes[i] = u8::from_str_radix(pair, 16).map_err(|_| {
                RpcError::InvalidParams("currency hex must be lowercase or uppercase hex".into())
            })?;
        }
        let code = IssuedCurrencyCode(bytes);
        code.validate()
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        return Ok(code);
    }
    if s.len() == 3
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        let mut bytes = [0u8; 20];
        bytes[..3].copy_from_slice(s.as_bytes());
        let code = IssuedCurrencyCode(bytes);
        code.validate()
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        return Ok(code);
    }
    Err(RpcError::InvalidParams(
        "currency must be exactly 40-byte hex or uppercase 3-character standard code".into(),
    ))
}

pub fn parse_issued_asset_id(params: &serde_json::Value) -> Result<IssuedAssetId, RpcError> {
    let (issuer, currency) = if let Some(obj) = params.as_object() {
        let issuer = parse_address_field(
            obj.get("issuer")
                .ok_or_else(|| RpcError::InvalidParams("issuer is required".into()))?,
        )?;
        let currency = parse_issued_currency_code(
            obj.get("currency")
                .ok_or_else(|| RpcError::InvalidParams("currency is required".into()))?,
        )?;
        (issuer, currency)
    } else if let Some(arr) = params.as_array() {
        if arr.len() != 2 {
            return Err(RpcError::InvalidParams(
                "asset id positional form requires [issuer, currency]".into(),
            ));
        }
        let issuer = parse_address_field(&arr[0])?;
        let currency = parse_issued_currency_code(&arr[1])?;
        (issuer, currency)
    } else {
        return Err(RpcError::InvalidParams(
            "asset id must be an object or [issuer, currency] array".into(),
        ));
    };
    let asset = IssuedAssetId { issuer, currency };
    asset
        .validate()
        .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
    Ok(asset)
}

pub fn parse_address_field(value: &serde_json::Value) -> Result<Address, RpcError> {
    let s = value
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams("address must be a hex string".into()))?;
    Address::from_hex(s).ok_or_else(|| RpcError::InvalidParams("invalid address hex".into()))
}

pub fn parse_holder_issuer_asset(
    params: &serde_json::Value,
) -> Result<(Address, IssuedAssetId), RpcError> {
    if let Some(obj) = params.as_object() {
        let holder = parse_address_field(
            obj.get("holder")
                .ok_or_else(|| RpcError::InvalidParams("holder is required".into()))?,
        )?;
        let issuer = parse_address_field(
            obj.get("issuer")
                .ok_or_else(|| RpcError::InvalidParams("issuer is required".into()))?,
        )?;
        let currency = parse_issued_currency_code(
            obj.get("currency")
                .ok_or_else(|| RpcError::InvalidParams("currency is required".into()))?,
        )?;
        let asset = IssuedAssetId { issuer, currency };
        asset
            .validate()
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        return Ok((holder, asset));
    }
    if let Some(arr) = params.as_array() {
        if arr.len() != 3 {
            return Err(RpcError::InvalidParams(
                "trust line query positional form requires [holder, issuer, currency]".into(),
            ));
        }
        let holder = parse_address_field(&arr[0])?;
        let issuer = parse_address_field(&arr[1])?;
        let currency = parse_issued_currency_code(&arr[2])?;
        let asset = IssuedAssetId { issuer, currency };
        asset
            .validate()
            .map_err(|e| RpcError::InvalidParams(e.to_string()))?;
        return Ok((holder, asset));
    }
    Err(RpcError::InvalidParams(
        "params must be object or positional array".into(),
    ))
}

fn parse_network_address(value: &serde_json::Value) -> Result<Address, RpcError> {
    let s = value
        .as_str()
        .ok_or_else(|| RpcError::InvalidParams("address must be a string".into()))?;
    Address::parse(s).ok_or_else(|| RpcError::InvalidParams("invalid address".into()))
}

/// Human-readable book side: `{ "type": "native_drc" }` or
/// `{ "type": "issued", "issuer": "<bech32|hex>", "currency": "USD"|40-hex }`.
pub fn parse_drc_book_asset(value: &serde_json::Value) -> Result<DrcBookAsset, RpcError> {
    let obj = value
        .as_object()
        .ok_or_else(|| RpcError::InvalidParams("book side must be an object".into()))?;
    let kind = obj
        .get("type")
        .and_then(|value| value.as_str())
        .ok_or_else(|| RpcError::InvalidParams("book side type is required".into()))?;
    match kind {
        "native_drc" => Ok(DrcBookAsset::NativeDrc),
        "issued" => {
            let issuer = parse_network_address(
                obj.get("issuer")
                    .ok_or_else(|| RpcError::InvalidParams("issued issuer is required".into()))?,
            )?;
            let currency =
                parse_issued_currency_code(obj.get("currency").ok_or_else(|| {
                    RpcError::InvalidParams("issued currency is required".into())
                })?)?;
            let asset = IssuedAssetId { issuer, currency };
            asset
                .validate()
                .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
            Ok(DrcBookAsset::Issued(asset))
        }
        _ => Err(RpcError::InvalidParams(
            "book side type must be native_drc or issued".into(),
        )),
    }
}

/// `agora_getDrcBookOffers` book. Accepts `pays`/`gets` or `taker_pays`/`taker_gets`.
pub fn parse_drc_offer_book(params: &serde_json::Value) -> Result<DrcOfferBook, RpcError> {
    let pays = params
        .get("pays")
        .or_else(|| params.get("taker_pays"))
        .ok_or_else(|| RpcError::InvalidParams("pays / taker_pays is required".into()))?;
    let gets = params
        .get("gets")
        .or_else(|| params.get("taker_gets"))
        .ok_or_else(|| RpcError::InvalidParams("gets / taker_gets is required".into()))?;
    let book = DrcOfferBook {
        pays: parse_drc_book_asset(pays)?,
        gets: parse_drc_book_asset(gets)?,
    };
    book.validate()
        .map_err(|error| RpcError::InvalidParams(error.to_string()))?;
    Ok(book)
}

#[cfg(test)]
mod currency_params_tests {
    use super::parse_issued_currency_code;
    use crate::RpcError;
    use serde_json::json;

    #[test]
    fn rejects_lowercase_standard_code() {
        let err = parse_issued_currency_code(&json!("usd")).unwrap_err();
        assert!(matches!(err, RpcError::InvalidParams(_)));
    }

    #[test]
    fn accepts_uppercase_standard_code() {
        parse_issued_currency_code(&json!("USD")).unwrap();
    }

    #[test]
    fn book_accepts_native_versus_issued_bech32() {
        let issuer =
            agora_types::Address::from_hex("aabbccddeeff00112233445566778899aabbccdd").unwrap();
        let book = super::parse_drc_offer_book(&json!({
            "pays": { "type": "native_drc" },
            "gets": {
                "type": "issued",
                "issuer": issuer.to_bech32_hrp("agoratest"),
                "currency": "USD"
            }
        }))
        .unwrap();
        assert!(book.pays.is_native());
        assert!(!book.gets.is_native());
    }

    #[test]
    fn book_rejects_identical_native_sides() {
        let err = super::parse_drc_offer_book(&json!({
            "taker_pays": { "type": "native_drc" },
            "taker_gets": { "type": "native_drc" }
        }))
        .unwrap_err();
        assert!(matches!(err, RpcError::InvalidParams(_)));
    }
}
