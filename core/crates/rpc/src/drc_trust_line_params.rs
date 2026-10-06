//! Strict public RPC currency parsing (no locale / lowercase normalization).

use agora_types::{Address, IssuedAssetId, IssuedCurrencyCode};

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
}
