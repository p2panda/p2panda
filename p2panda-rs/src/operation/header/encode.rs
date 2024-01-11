// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::body::EncodedBody;
use crate::operation::header::error::EncodeHeaderError;
use crate::operation::header::{EncodedHeader, Header, HeaderExtension};
use crate::operation::OperationVersion;

use super::header::DocumentLinks;
use super::{seq_num, SeqNum};

pub fn sign_header(
    timestamp: u64,
    seq_num: SeqNum,
    backlink: Option<Hash>,
    document_links: Option<DocumentLinks>,
    tombstone: bool,
    extension: HeaderExtension,
    payload: &EncodedBody,
    key_pair: &KeyPair,
) -> Result<Header, EncodeHeaderError> {
    let mut header = Header(
        OperationVersion::V1,
        key_pair.public_key(),
        payload.hash(),
        payload.size(),
        timestamp,
        seq_num,
        backlink,
        document_links,
        tombstone,
        extension,
        None,
    );

    // @TODO: we're signing the CBOR encoded bytes here, not convinced this is the desired approach.
    let unsigned_encoded_header = encode_header(&header)?;
    header.10 = Some(key_pair.sign(&unsigned_encoded_header.to_bytes()));

    Ok(header)
}

pub fn encode_header(header: &Header) -> Result<EncodedHeader, EncodeHeaderError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&header, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeHeaderError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeHeaderError::EncoderFailed(err),
    })?;

    Ok(EncodedHeader::from_bytes(&cbor_bytes))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::body::encode::encode_body;
    use crate::operation::body::Body;
    use crate::operation::header::encode::encode_header;
    use crate::operation::header::HeaderBuilder;
    use crate::operation::OperationValue;
    use crate::operation::traits::Verifiable;
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        document_id, document_view_id, hash, key_pair, operation_fields, random_key_pair, schema_id,
    };

    #[rstest]
    fn sign_and_encode_header(
        schema_id: SchemaId,
        #[from(operation_fields)] fields: Vec<(&str, OperationValue)>,
        document_id: DocumentId,
        #[from(hash)] backlink: Hash,
        #[from(document_view_id)] previous: DocumentViewId,
        key_pair: KeyPair,
        #[from(random_key_pair)] incorrect_key_pair: KeyPair,
    ) {
        let body = Body(Some(fields.into()));
        let encoded_body = encode_body(&body).unwrap();
        let result = HeaderBuilder::new()
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&previous)
            .timestamp(1703027623)
            .sign(&encoded_body, &key_pair);

        assert!(result.is_ok());
        let header = result.unwrap();

        let result = encode_header(&header);
        assert!(result.is_ok());
        let encoded_header = result.unwrap();

        let pass = key_pair
            .public_key()
            .verify(&encoded_header.unsigned_bytes(), &header.signature());
        assert!(pass);

        let pass = incorrect_key_pair
            .public_key()
            .verify(&encoded_header.unsigned_bytes(), &header.signature());
        assert!(!pass)
    }
}
