// Copyright (c) 2022 MASSA LABS <info@massa.net>

use crate::constants::AMOUNT_DECIMAL_FACTOR;
use crate::ModelsError;
use massa_serialization::{Deserializer, SerializeError, Serializer};
use massa_serialization::{U64VarIntDeserializer, U64VarIntSerializer};
use nom::error::{context, ContextError, ParseError};
use nom::IResult;
use rust_decimal::prelude::*;
use serde::de::Unexpected;
use std::fmt;
use std::ops::Bound;
use std::str::FromStr;

/// A structure representing a decimal Amount of coins with safe operations
/// this allows ensuring that there is never an uncontrolled overflow or precision loss
/// while providing a convenient decimal interface for users
/// The underlying `u64` raw representation if a fixed-point value with factor `AMOUNT_DECIMAL_FACTOR`
/// The minimal value is 0 and the maximal value is 18446744073.709551615
#[derive(Clone, Copy, Debug, PartialEq, Eq, Ord, PartialOrd, Default)]
pub struct Amount(u64);

impl Amount {
    /// Create a zero Amount
    pub const fn zero() -> Self {
        Self(0)
    }

    /// Obtains the underlying raw `u64` representation
    /// Warning: do not use this unless you know what you are doing
    /// because the raw value does not take the `AMOUNT_DECIMAL_FACTOR` into account.
    pub fn to_raw(&self) -> u64 {
        self.0
    }

    /// constructs an `Amount` from the underlying raw `u64` representation
    /// Warning: do not use this unless you know what you are doing
    /// because the raw value does not take the `AMOUNT_DECIMAL_FACTOR` into account
    /// In most cases, you should be using `Amount::from_str("11.23")`
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// safely add self to another amount, saturating the result on overflow
    #[must_use]
    pub fn saturating_add(self, amount: Amount) -> Self {
        Amount(self.0.saturating_add(amount.0))
    }

    /// safely subtract another amount from self, saturating the result on underflow
    #[must_use]
    pub fn saturating_sub(self, amount: Amount) -> Self {
        Amount(self.0.saturating_sub(amount.0))
    }

    /// returns true if the amount is zero
    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// safely subtract another amount from self, returning None on underflow
    /// ```
    /// # use massa_models::Amount;
    /// # use std::str::FromStr;
    /// let amount_1 : Amount = Amount::from_str("42").unwrap();
    /// let amount_2 : Amount = Amount::from_str("7").unwrap();
    /// let res : Amount = amount_1.checked_sub(amount_2).unwrap();
    /// assert_eq!(res, Amount::from_str("35").unwrap())
    /// ```
    pub fn checked_sub(self, amount: Amount) -> Option<Self> {
        self.0.checked_sub(amount.0).map(Amount)
    }

    /// safely add self to another amount, returning None on overflow
    /// ```
    /// # use massa_models::Amount;
    /// # use std::str::FromStr;
    /// let amount_1 : Amount = Amount::from_str("42").unwrap();
    /// let amount_2 : Amount = Amount::from_str("7").unwrap();
    /// let res : Amount = amount_1.checked_add(amount_2).unwrap();
    /// assert_eq!(res, Amount::from_str("49").unwrap())
    /// ```
    pub fn checked_add(self, amount: Amount) -> Option<Self> {
        self.0.checked_add(amount.0).map(Amount)
    }

    /// safely multiply self with a `u64`, returning None on overflow
    /// ```
    /// # use massa_models::Amount;
    /// # use std::str::FromStr;
    /// let amount_1 : Amount = Amount::from_str("42").unwrap();
    /// let res : Amount = amount_1.checked_mul_u64(7).unwrap();
    /// assert_eq!(res, Amount::from_str("294").unwrap())
    /// ```
    pub fn checked_mul_u64(self, factor: u64) -> Option<Self> {
        self.0.checked_mul(factor).map(Amount)
    }

    /// safely multiply self with a `u64`, saturating the result on overflow
    /// ```
    /// # use massa_models::Amount;
    /// # use std::str::FromStr;
    /// let amount_1 : Amount = Amount::from_str("42").unwrap();
    /// let res : Amount = amount_1.saturating_mul_u64(7);
    /// assert_eq!(res, Amount::from_str("294").unwrap());
    /// ```
    #[must_use]
    pub fn saturating_mul_u64(self, factor: u64) -> Self {
        Amount(self.0.saturating_mul(factor))
    }

    /// safely divide self by a `u64`, returning None if the factor is zero
    /// ```
    /// # use massa_models::Amount;
    /// # use std::str::FromStr;
    /// let amount_1 : Amount = Amount::from_str("42").unwrap();
    /// let res : Amount = amount_1.checked_div_u64(7).unwrap();
    /// assert_eq!(res, Amount::from_str("6").unwrap());
    /// ```
    pub fn checked_div_u64(self, factor: u64) -> Option<Self> {
        self.0.checked_div(factor).map(Amount)
    }
}

/// display an Amount in decimal string form (like "10.33")
///
/// ```
/// # use massa_models::Amount;
/// # use std::str::FromStr;
/// let value = Amount::from_str("11.111").unwrap();
/// assert_eq!(format!("{}", value), "11.111")
/// ```
impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let res_string = Decimal::from_u64(self.0)
            .unwrap() // will never panic
            .checked_div(AMOUNT_DECIMAL_FACTOR.into()) // will never panic
            .unwrap() // will never panic
            .to_string();
        write!(f, "{}", res_string)
    }
}

/// build an Amount from decimal string form (like "10.33")
/// note that this will fail if the string format is invalid
/// or if the conversion would cause an overflow, underflow or precision loss
///
/// ```
/// # use massa_models::Amount;
/// # use std::str::FromStr;
/// assert!(Amount::from_str("11.1").is_ok());
/// assert!(Amount::from_str("11.1111111111111111111111").is_err());
/// assert!(Amount::from_str("1111111111111111111111").is_err());
/// assert!(Amount::from_str("-11.1").is_err());
/// assert!(Amount::from_str("abc").is_err());
/// ```
impl FromStr for Amount {
    type Err = ModelsError;

    fn from_str(str_amount: &str) -> Result<Self, Self::Err> {
        let res = Decimal::from_str(str_amount)
            .map_err(|err| ModelsError::AmountParseError(err.to_string()))?
            .checked_mul(AMOUNT_DECIMAL_FACTOR.into())
            .ok_or_else(|| ModelsError::AmountParseError("amount is too large".to_string()))?;
        if res.is_sign_negative() {
            return Err(ModelsError::AmountParseError(
                "amounts cannot be strictly negative".to_string(),
            ));
        }
        if !res.fract().is_zero() {
            return Err(ModelsError::AmountParseError(format!(
                "amounts cannot be more precise than 1/{}",
                AMOUNT_DECIMAL_FACTOR
            )));
        }
        let res = res.to_u64().ok_or_else(|| {
            ModelsError::AmountParseError(
                "amount is too large to be represented as u64".to_string(),
            )
        })?;
        Ok(Amount(res))
    }
}

/// Serializer for amount
pub struct AmountSerializer {
    u64_serializer: U64VarIntSerializer,
}

impl AmountSerializer {
    /// Create a new `AmountSerializer`
    pub fn new(min_amount: Bound<u64>, max_amount: Bound<u64>) -> Self {
        Self {
            u64_serializer: U64VarIntSerializer::new(min_amount, max_amount),
        }
    }
}

impl Serializer<Amount> for AmountSerializer {
    /// ```
    /// use massa_models::{Amount, AmountSerializer};
    /// use massa_serialization::Serializer;
    /// use std::str::FromStr;
    /// use std::ops::Bound::Included;
    ///
    /// let amount = Amount::from_str("11.111").unwrap();
    /// let serializer = AmountSerializer::new(Included(0), Included(u64::MAX));
    /// let mut serialized = vec![];
    /// serializer.serialize(&amount, &mut serialized).unwrap();
    /// ```
    fn serialize(&self, value: &Amount, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        self.u64_serializer.serialize(&value.0, buffer)
    }
}

/// Deserializer for amount
pub struct AmountDeserializer {
    u64_deserializer: U64VarIntDeserializer,
}

impl AmountDeserializer {
    /// Create a new `AmountDeserializer`
    pub fn new(min_amount: Bound<u64>, max_amount: Bound<u64>) -> Self {
        Self {
            u64_deserializer: U64VarIntDeserializer::new(min_amount, max_amount),
        }
    }
}

impl Deserializer<Amount> for AmountDeserializer {
    /// ```
    /// use massa_models::{Amount, AmountSerializer, AmountDeserializer};
    /// use massa_serialization::{Serializer, Deserializer, DeserializeError};
    /// use std::str::FromStr;
    /// use std::ops::Bound::Included;
    ///
    /// let amount = Amount::from_str("11.111").unwrap();
    /// let serializer = AmountSerializer::new(Included(0), Included(u64::MAX));
    /// let deserializer = AmountDeserializer::new(Included(0), Included(u64::MAX));
    /// let mut serialized = vec![];
    /// serializer.serialize(&amount, &mut serialized).unwrap();
    /// let (rest, amount_deser) = deserializer.deserialize::<DeserializeError>(&serialized).unwrap();
    /// assert!(rest.is_empty());
    /// assert_eq!(amount_deser, amount);
    /// ```
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], Amount, E> {
        context("Failed Amount deserialization", |input| {
            let (rest, raw) = self.u64_deserializer.deserialize(input)?;
            Ok((rest, Amount::from_raw(raw)))
        })(buffer)
    }
}

impl<'de> serde::Deserialize<'de> for Amount {
    fn deserialize<D>(deserializer: D) -> Result<Amount, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        deserializer.deserialize_str(AmountVisitor)
    }
}

struct AmountVisitor;

impl<'de> serde::de::Visitor<'de> for AmountVisitor {
    type Value = Amount;

    fn visit_str<E>(self, value: &str) -> Result<Amount, E>
    where
        E: serde::de::Error,
    {
        Amount::from_str(value).map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
    }

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(
            formatter,
            "an Amount type representing a fixed-point currency amount"
        )
    }
}

impl serde::Serialize for Amount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
