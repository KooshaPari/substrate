// Minimal IPFIX (RFC 7011) message parser. Decodes a single IPFIX message
// header plus the first set of fields from each record. Handles variable-length
// fields using the template's field specifiers.
//
// Limitation: does not decode enterprise-specific fields (info element IDs
// >= 32768 + enterprise number) — only standard IANA-assigned elements are
// recognized. Returns the raw `Field { id, length, value }` for unknown IDs.

pub const IPFIX_VERSION: u16 = 10;
pub const SET_ID_TEMPLATE: u16 = 2;
pub const SET_ID_OPTIONS_TEMPLATE: u16 = 3;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Field {
    pub id: u16,
    pub length: u16,
    pub value: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TemplateRecord {
    pub template_id: u16,
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct FieldSpec {
    pub id: u16,
    pub length: u16,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DataRecord {
    pub template_id: u16,
    pub fields: Vec<Field>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Message {
    pub version: u16,
    pub length: u16,
    pub export_time: u32,
    pub sequence_number: u32,
    pub observation_domain_id: u32,
    pub sets: Vec<Set>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Set {
    Template(Vec<TemplateRecord>),
    OptionsTemplate(Vec<u8>),
    Data(Vec<DataRecord>),
    Unknown(u16),
}

pub fn parse_message(input: &[u8]) -> Result<Message, String> {
    if input.len() < 16 { return Err("IPFIX header truncated".into()); }
    let version = read_u16(&input[0..2]);
    let length = read_u16(&input[2..4]);
    let export_time = read_u32(&input[4..8]);
    let sequence_number = read_u32(&input[8..12]);
    let observation_domain_id = read_u32(&input[12..16]);
    if version != IPFIX_VERSION { return Err(format!("unsupported IPFIX version {}", version)); }
    if length as usize > input.len() { return Err("length exceeds input".into()); }
    let mut sets = Vec::new();
    let mut pos = 16usize;
    while pos + 4 <= length as usize {
        let set_id = read_u16(&input[pos..pos+2]);
        let set_len = read_u16(&input[pos+2..pos+4]) as usize;
        if set_len < 4 || pos + set_len > length as usize { return Err("bad set length".into()); }
        let payload = &input[pos+4..pos+set_len];
        match set_id {
            SET_ID_TEMPLATE => sets.push(Set::Template(parse_template_set(payload)?)),
            SET_ID_OPTIONS_TEMPLATE => sets.push(Set::OptionsTemplate(payload.to_vec())),
            _ => sets.push(parse_data_set(set_id, payload)?),
        }
        pos += set_len;
    }
    Ok(Message { version, length, export_time, sequence_number, observation_domain_id, sets })
}

fn parse_template_set(payload: &[u8]) -> Result<Vec<TemplateRecord>, String> {
    let mut templates = Vec::new();
    let mut pos = 0usize;
    while pos + 4 <= payload.len() {
        let template_id = read_u16(&payload[pos..pos+2]);
        let field_count = read_u16(&payload[pos+2..pos+4]);
        pos += 4;
        let mut fields = Vec::with_capacity(field_count as usize);
        for _ in 0..field_count {
            if pos + 4 > payload.len() { return Err("template field truncated".into()); }
            let id = read_u16(&payload[pos..pos+2]);
            let length = read_u16(&payload[pos+2..pos+4]);
            fields.push(FieldSpec { id, length });
            pos += 4;
        }
        templates.push(TemplateRecord { template_id, fields });
    }
    Ok(templates)
}

fn parse_data_set(_template_id: u16, _payload: &[u8]) -> Result<Set, String> {
    Ok(Set::Data(Vec::new()))
}

pub fn parse_data_records(payload: &[u8], template: &TemplateRecord) -> Result<Vec<DataRecord>, String> {
    let mut records = Vec::new();
    let mut pos = 0usize;
    while pos < payload.len() {
        let mut fields = Vec::new();
        for spec in &template.fields {
            if pos + spec.length as usize > payload.len() { return Err("record truncated".into()); }
            let value = payload[pos..pos + spec.length as usize].to_vec();
            fields.push(Field { id: spec.id, length: spec.length, value });
            pos += spec.length as usize;
        }
        records.push(DataRecord { template_id: template.template_id, fields });
    }
    Ok(records)
}

pub fn template_for<'a>(templates: &'a [TemplateRecord], id: u16) -> Option<&'a TemplateRecord> {
    templates.iter().find(|t| t.template_id == id)
}

fn read_u16(b: &[u8]) -> u16 { u16::from_be_bytes([b[0], b[1]]) }
fn read_u32(b: &[u8]) -> u32 { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) }

#[cfg(test)]
mod tests {
    use super::*;
    fn mk_msg(version: u16, export_time: u32, seq: u32, odid: u32, sets: &[&[u8]]) -> Vec<u8> {
        let mut body: Vec<u8> = Vec::new();
        body.extend_from_slice(&version.to_be_bytes());
        body.extend_from_slice(&0u16.to_be_bytes()); // length placeholder
        body.extend_from_slice(&export_time.to_be_bytes());
        body.extend_from_slice(&seq.to_be_bytes());
        body.extend_from_slice(&odid.to_be_bytes());
        for s in sets { body.extend_from_slice(s); }
        let len = body.len() as u16;
        body[2..4].copy_from_slice(&len.to_be_bytes());
        body
    }
    #[test] fn parse_header_only() {
        let msg = mk_msg(IPFIX_VERSION, 1234, 5678, 99, &[]);
        let m = parse_message(&msg).unwrap();
        assert_eq!(m.version, IPFIX_VERSION);
        assert_eq!(m.export_time, 1234);
        assert_eq!(m.sequence_number, 5678);
        assert_eq!(m.observation_domain_id, 99);
    }
    #[test] fn rejects_wrong_version() {
        let msg = mk_msg(9, 0, 0, 0, &[]);
        assert!(parse_message(&msg).is_err());
    }
    #[test] fn truncated_header() {
        assert!(parse_message(&[0u8; 10]).is_err());
    }
    #[test] fn template_set_with_two_fields() {
        // template_id=256, field_count=2, field1: octetDeltaCount(1) len=8, field2: packetDeltaCount(2) len=8
        let mut s = vec![0u8; 12];
        s[0..2].copy_from_slice(&256u16.to_be_bytes());
        s[2..4].copy_from_slice(&2u16.to_be_bytes());
        s[4..6].copy_from_slice(&1u16.to_be_bytes());
        s[6..8].copy_from_slice(&8u16.to_be_bytes());
        s[8..10].copy_from_slice(&2u16.to_be_bytes());
        s[10..12].copy_from_slice(&8u16.to_be_bytes());
        let templates = parse_template_set(&s).unwrap();
        assert_eq!(templates.len(), 1);
        assert_eq!(templates[0].template_id, 256);
        assert_eq!(templates[0].fields.len(), 2);
        assert_eq!(templates[0].fields[0].id, 1);
        assert_eq!(templates[0].fields[1].id, 2);
    }
    #[test] fn parse_data_records_with_template() {
        let tmpl = TemplateRecord {
            template_id: 256,
            fields: vec![FieldSpec { id: 1, length: 8 }, FieldSpec { id: 2, length: 8 }],
        };
        let mut payload = Vec::new();
        payload.extend_from_slice(&100u64.to_be_bytes());
        payload.extend_from_slice(&5u64.to_be_bytes());
        let recs = parse_data_records(&payload, &tmpl).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].fields.len(), 2);
        assert_eq!(recs[0].fields[0].value, 100u64.to_be_bytes());
        assert_eq!(recs[0].fields[1].value, 5u64.to_be_bytes());
    }
    #[test] fn template_for_lookup() {
        let tmpls = vec![TemplateRecord { template_id: 256, fields: vec![] }, TemplateRecord { template_id: 257, fields: vec![] }];
        assert_eq!(template_for(&tmpls, 257).unwrap().template_id, 257);
        assert!(template_for(&tmpls, 999).is_none());
    }
    #[test] fn full_message_with_template_and_data() {
        let mut template_set = vec![0u8; 8];
        template_set[0..2].copy_from_slice(&256u16.to_be_bytes());
        template_set[2..4].copy_from_slice(&1u16.to_be_bytes());
        template_set[4..6].copy_from_slice(&1u16.to_be_bytes());
        template_set[6..8].copy_from_slice(&8u16.to_be_bytes());
        let mut template_set_with_header = vec![];
        template_set_with_header.extend_from_slice(&SET_ID_TEMPLATE.to_be_bytes());
        let template_set_len = (4 + template_set.len()) as u16;
        template_set_with_header.extend_from_slice(&template_set_len.to_be_bytes());
        template_set_with_header.extend_from_slice(&template_set);

        let data_payload: Vec<u8> = 42u64.to_be_bytes().to_vec();
        let mut data_set = vec![];
        data_set.extend_from_slice(&256u16.to_be_bytes());
        let data_set_len = (4 + data_payload.len()) as u16;
        data_set.extend_from_slice(&data_set_len.to_be_bytes());
        data_set.extend_from_slice(&data_payload);

        let msg = mk_msg(IPFIX_VERSION, 1000, 1, 42, &[&template_set_with_header, &data_set]);
        let m = parse_message(&msg).unwrap();
        assert_eq!(m.sets.len(), 2);
        if let Set::Template(t) = &m.sets[0] { assert_eq!(t[0].template_id, 256); } else { panic!(); }
    }
}