// Copyright (c) 2022 MASSA LABS <info@massa.net>

//! This file defines the structure representing an entry in the `FinalLedger`

use crate::ledger_changes::LedgerEntryUpdate;
use crate::types::{Applicable, SetOrDelete};
use massa_hash::HASH_SIZE_BYTES;
use massa_hash::{Hash, HashDeserializer};
use massa_models::amount::{AmountDeserializer, AmountSerializer};
use massa_models::{
    array_from_slice, Amount, DeserializeVarInt, ModelsError, SerializeVarInt, VecU8Deserializer,
    VecU8Serializer,
};
use massa_models::{DeserializeCompact, SerializeCompact};
use massa_serialization::{
    Deserializer, SerializeError, Serializer, U64VarIntDeserializer, U64VarIntSerializer,
};
use nom::error::{context, ContextError, ParseError};
use nom::multi::length_count;
use nom::sequence::tuple;
use nom::{IResult, Parser};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::Bound::Included;

/// Structure defining an entry associated to an address in the `FinalLedger`
#[derive(Default, Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct LedgerEntry {
    /// The parallel balance of that entry.
    /// See lib.rs for an explanation on parallel vs sequential balances.
    pub parallel_balance: Amount,

    /// Executable bytecode
    pub bytecode: Vec<u8>,

    /// A key-value store associating a hash to arbitrary bytes
    pub datastore: BTreeMap<Hash, Vec<u8>>,
}

/// Serializer for `Datastore` field in `LedgerEntry`
pub struct DatastoreSerializer {
    u64_serializer: U64VarIntSerializer,
    value_serializer: VecU8Serializer,
}

impl DatastoreSerializer {
    /// Creates a new `DatastoreSerializer`
    pub fn new() -> Self {
        Self {
            u64_serializer: U64VarIntSerializer::new(Included(u64::MIN), Included(u64::MAX)),
            value_serializer: VecU8Serializer::new(Included(u64::MIN), Included(u64::MAX)),
        }
    }
}

impl Serializer<BTreeMap<Hash, Vec<u8>>> for DatastoreSerializer {
    fn serialize(
        &self,
        value: &BTreeMap<Hash, Vec<u8>>,
        buffer: &mut Vec<u8>,
    ) -> Result<(), SerializeError> {
        let entry_count: u64 = value.len().try_into().map_err(|err| {
            SerializeError::GeneralError(format!(
                "too many entries in ConsensusLedgerSubset: {}",
                err
            ))
        })?;
        self.u64_serializer.serialize(&entry_count, buffer)?;
        for (key, value) in value.iter() {
            buffer.extend(key.to_bytes());
            self.value_serializer.serialize(value, buffer)?;
        }
        Ok(())
    }
}

/// Deserializer for `Datastore` field in `LedgerEntry`
pub struct DatastoreDeserializer {
    u64_deserializer: U64VarIntDeserializer,
    hash_deserializer: HashDeserializer,
    value_deserializer: VecU8Deserializer,
}

impl DatastoreDeserializer {
    /// Creates a new `DatastoreDeserializer`
    pub fn new() -> Self {
        Self {
            u64_deserializer: U64VarIntDeserializer::new(Included(u64::MIN), Included(u64::MAX)),
            hash_deserializer: HashDeserializer::new(),
            value_deserializer: VecU8Deserializer::new(Included(u64::MIN), Included(u64::MAX)),
        }
    }
}

impl Deserializer<BTreeMap<Hash, Vec<u8>>> for DatastoreDeserializer {
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], BTreeMap<Hash, Vec<u8>>, E> {
        context(
            "Failed Datastore deserialization",
            length_count(
                context("Failed length deserialization", |input| {
                    self.u64_deserializer.deserialize(input)
                }),
                tuple((
                    |input| self.hash_deserializer.deserialize(input),
                    |input| self.value_deserializer.deserialize(input),
                )),
            ),
        )
        .map(|elements| elements.into_iter().collect())
        .parse(buffer)
    }
}

/// Serializer for `LedgerEntry`
pub struct LedgerEntrySerializer {
    amount_serializer: AmountSerializer,
    vec_u8_serializer: VecU8Serializer,
    datastore_serializer: DatastoreSerializer,
}

impl LedgerEntrySerializer {
    /// Creates a new `LedgerEntrySerializer`
    pub fn new() -> Self {
        Self {
            vec_u8_serializer: VecU8Serializer::new(Included(u64::MIN), Included(u64::MAX)),
            amount_serializer: AmountSerializer::new(Included(u64::MIN), Included(u64::MAX)),
            datastore_serializer: DatastoreSerializer::new(),
        }
    }
}

impl Default for LedgerEntrySerializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Serializer<LedgerEntry> for LedgerEntrySerializer {
    /// ```
    /// use massa_serialization::Serializer;
    /// use std::collections::BTreeMap;
    /// use std::str::FromStr;
    /// use massa_models::Amount;
    /// use massa_ledger_exports::{LedgerEntry, LedgerEntrySerializer};
    /// use massa_hash::Hash;
    ///
    /// let hash = Hash::compute_from(&"hello world".as_bytes());
    /// let mut store = BTreeMap::new();
    /// store.insert(hash, vec![1, 2, 3]);
    /// let amount = Amount::from_str("1").unwrap();
    /// let bytecode = vec![1, 2, 3];
    /// let ledger_entry = LedgerEntry {
    ///    parallel_balance: amount,
    ///    bytecode,
    ///    datastore: store,
    /// };
    /// let mut serialized = Vec::new();
    /// let serializer = LedgerEntrySerializer::new();
    /// serializer.serialize(&ledger_entry, &mut serialized).unwrap();
    /// ```
    fn serialize(&self, value: &LedgerEntry, buffer: &mut Vec<u8>) -> Result<(), SerializeError> {
        self.amount_serializer
            .serialize(&value.parallel_balance, buffer)?;
        self.vec_u8_serializer.serialize(&value.bytecode, buffer)?;
        self.datastore_serializer
            .serialize(&value.datastore, buffer)?;
        Ok(())
    }
}

/// Deserializer for `LedgerEntry`
pub struct LedgerEntryDeserializer {
    amount_deserializer: AmountDeserializer,
    vec_u8_deserializer: VecU8Deserializer,
    datastore_deserializer: DatastoreDeserializer,
}

impl LedgerEntryDeserializer {
    /// Creates a new `LedgerEntryDeserializer`
    pub fn new() -> Self {
        Self {
            amount_deserializer: AmountDeserializer::new(Included(u64::MIN), Included(u64::MAX)),
            vec_u8_deserializer: VecU8Deserializer::new(Included(u64::MIN), Included(u64::MAX)),
            datastore_deserializer: DatastoreDeserializer::new(),
        }
    }
}

impl Default for LedgerEntryDeserializer {
    fn default() -> Self {
        Self::new()
    }
}

impl Deserializer<LedgerEntry> for LedgerEntryDeserializer {
    /// ```
    /// use massa_serialization::{Deserializer, Serializer, DeserializeError};
    /// use std::collections::BTreeMap;
    /// use std::str::FromStr;
    /// use massa_models::Amount;
    /// use massa_ledger_exports::{LedgerEntry, LedgerEntrySerializer, LedgerEntryDeserializer};
    /// use massa_hash::Hash;
    ///
    /// let hash = Hash::compute_from(&"hello world".as_bytes());
    /// let mut store = BTreeMap::new();
    /// store.insert(hash, vec![1, 2, 3]);
    /// let amount = Amount::from_str("1").unwrap();
    /// let bytecode = vec![1, 2, 3];
    /// let ledger_entry = LedgerEntry {
    ///    parallel_balance: amount,
    ///    bytecode,
    ///    datastore: store,
    /// };
    /// let mut serialized = Vec::new();
    /// let serializer = LedgerEntrySerializer::new();
    /// let deserializer = LedgerEntryDeserializer::new();
    /// serializer.serialize(&ledger_entry, &mut serialized).unwrap();
    /// let (rest, ledger_entry_deser) = deserializer.deserialize::<DeserializeError>(&serialized).unwrap();
    /// assert!(rest.is_empty());
    /// assert_eq!(ledger_entry, ledger_entry_deser);
    /// ```
    fn deserialize<'a, E: ParseError<&'a [u8]> + ContextError<&'a [u8]>>(
        &self,
        buffer: &'a [u8],
    ) -> IResult<&'a [u8], LedgerEntry, E> {
        context(
            "Failed LedgerEntry deserialization",
            tuple((
                context("Failed parallel_balance deserialization", |input| {
                    self.amount_deserializer.deserialize(input)
                }),
                context("Failed bytecode deserialization", |input| {
                    self.vec_u8_deserializer.deserialize(input)
                }),
                context("Failed datastore deserialization", |input| {
                    self.datastore_deserializer.deserialize(input)
                }),
            )),
        )
        .map(|(parallel_balance, bytecode, datastore)| LedgerEntry {
            parallel_balance,
            bytecode,
            datastore,
        })
        .parse(buffer)
    }
}

/// A `LedgerEntryUpdate` can be applied to a `LedgerEntry`
impl Applicable<LedgerEntryUpdate> for LedgerEntry {
    fn apply(&mut self, update: LedgerEntryUpdate) {
        // apply updates to the parallel balance
        update.parallel_balance.apply_to(&mut self.parallel_balance);

        // apply updates to the executable bytecode
        update.bytecode.apply_to(&mut self.bytecode);

        // iterate over all datastore updates
        for (key, value_update) in update.datastore {
            match value_update {
                // this update sets a new value to a datastore entry
                SetOrDelete::Set(v) => {
                    // insert the new value in the datastore,
                    // replacing any existing value
                    self.datastore.insert(key, v);
                }

                // this update deletes a datastore entry
                SetOrDelete::Delete => {
                    // remove that entry from the datastore if it exists
                    self.datastore.remove(&key);
                }
            }
        }
    }
}

/// Allow serializing the `LedgerEntry` into a compact binary representation
impl SerializeCompact for LedgerEntry {
    fn to_bytes_compact(&self) -> Result<Vec<u8>, massa_models::ModelsError> {
        let mut res: Vec<u8> = Vec::new();

        // parallel balance
        res.extend(self.parallel_balance.to_bytes_compact()?);

        // bytecode length
        let bytecode_len: u64 = self.bytecode.len().try_into().map_err(|_| {
            ModelsError::SerializeError("could not convert bytecode size to u64".into())
        })?;
        res.extend(bytecode_len.to_varint_bytes());

        // bytecode
        res.extend(&self.bytecode);

        // datastore length
        let datastore_len: u64 = self.datastore.len().try_into().map_err(|_| {
            ModelsError::SerializeError("could not convert datastore size to u64".into())
        })?;
        res.extend(datastore_len.to_varint_bytes());

        // datastore
        for (key, value) in &self.datastore {
            // key
            res.extend(key.to_bytes());

            // value length
            let value_len: u64 = value.len().try_into().map_err(|_| {
                ModelsError::SerializeError("could not convert datastore value size to u64".into())
            })?;
            res.extend(value_len.to_varint_bytes());

            // value
            res.extend(value);
        }

        Ok(res)
    }
}

/// Allow deserializing a `LedgerEntry` from its compact binary representation
impl DeserializeCompact for LedgerEntry {
    fn from_bytes_compact(buffer: &[u8]) -> Result<(Self, usize), massa_models::ModelsError> {
        let mut cursor = 0usize;

        // parallel balance
        let (parallel_balance, delta) = Amount::from_bytes_compact(&buffer[cursor..])?;
        cursor += delta;

        // bytecode length
        let (bytecode_len, delta) = u64::from_varint_bytes(&buffer[cursor..])?;
        let bytecode_len: usize = bytecode_len.try_into().map_err(|_| {
            ModelsError::SerializeError("could not convert bytecode size to usize".into())
        })?;
        //TODO cap bytecode length https://github.com/massalabs/massa/issues/1200
        cursor += delta;

        // bytecode
        let bytecode = if let Some(slice) = buffer.get(cursor..(cursor + (bytecode_len as usize))) {
            cursor += bytecode_len as usize;
            slice.to_vec()
        } else {
            return Err(ModelsError::DeserializeError(
                "could not deserialize ledger entry bytecode: buffer too small".into(),
            ));
        };

        // datastore length
        let (datastore_len, delta) = u64::from_varint_bytes(&buffer[cursor..])?;
        let datastore_len: usize = datastore_len.try_into().map_err(|_| {
            ModelsError::SerializeError("could not convert datastore size to usize".into())
        })?;
        //TODO cap datastore length https://github.com/massalabs/massa/issues/1200
        cursor += delta;

        // datastore entries
        let mut datastore: BTreeMap<Hash, Vec<u8>> = BTreeMap::new();
        for _ in 0..datastore_len {
            // key
            let key = Hash::from_bytes(&array_from_slice(&buffer[cursor..])?);
            cursor += HASH_SIZE_BYTES;

            // value length
            let (value_len, delta) = u64::from_varint_bytes(&buffer[cursor..])?;
            let value_len: usize = value_len.try_into().map_err(|_| {
                ModelsError::SerializeError(
                    "could not convert datastore entry value size to usize".into(),
                )
            })?;
            //TODO cap value length https://github.com/massalabs/massa/issues/1200
            cursor += delta;

            // value
            let value = if let Some(slice) = buffer.get(cursor..(cursor + (value_len as usize))) {
                cursor += value_len as usize;
                slice.to_vec()
            } else {
                return Err(ModelsError::DeserializeError(
                    "could not deserialize ledger entry datastore value: buffer too small".into(),
                ));
            };

            datastore.insert(key, value);
        }

        Ok((
            LedgerEntry {
                parallel_balance,
                bytecode,
                datastore,
            },
            cursor,
        ))
    }
}
