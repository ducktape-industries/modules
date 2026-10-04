use super::*;

/// 50,000 bytes of every value, so an array of integers would spend one
/// byte on some and two on the rest.
fn mixed() -> Vec<u8> {
    (0..50_000u32).map(|at| (at * 31 % 256) as u8).collect()
}

/// `value` holds `payload`, and on the wire that is a `bin`: its marker,
/// its length and the bytes as they are, in a value its own fields larger.
fn crosses_as_bin<T>(value: T, payload: &[u8])
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let wire = encode(&value);
    let header: &[u8] = match payload.len() {
        len @ 0..=0xff => &[0xc4, len as u8],
        len => &[0xc5, (len >> 8) as u8, len as u8],
    };
    let at = memchr::memmem::find(&wire, payload).expect("the bytes cross as they are");
    assert_eq!(&wire[at - header.len()..at], header);
    assert!(
        wire.len() <= payload.len() + 64,
        "{} bytes cross as {}",
        payload.len(),
        wire.len()
    );
    assert_eq!(decode::<T>(&wire).unwrap(), value);
}

#[test]
fn a_reply_crosses_as_bin() {
    let reply = Event::Response {
        id: 1,
        result: Ok(mixed()),
        done: true,
    };
    // {9: [1, {0: bin}, true]}: ten bytes around the payload.
    assert_eq!(encode(&reply).len(), 50_010);
    crosses_as_bin(reply, &mixed());
}

#[test]
fn a_request_payload_crosses_as_bin() {
    let request = || Request {
        id: 1,
        kind: "module.query".into(),
        payload: mixed(),
    };
    crosses_as_bin(request(), &mixed());
    crosses_as_bin(
        Frame {
            requests: vec![request()],
            ..Default::default()
        },
        &mixed(),
    );
    // The envelope of a node method, when it crosses the tree's codec.
    crosses_as_bin(
        methods::Call {
            target: "chat".into(),
            body: mixed(),
        },
        &mixed(),
    );
}

#[test]
fn a_picture_crosses_as_bin() {
    crosses_as_bin(ImageData::Encoded(mixed()), &mixed());
    crosses_as_bin(
        ImageData::Rgba {
            width: 125,
            height: 100,
            pixels: mixed(),
        },
        &mixed(),
    );
}

#[test]
fn an_svg_crosses_as_bin() {
    crosses_as_bin(picture(Some(mixed())), &mixed());
    let held = SvgSource::Data {
        hash: 7,
        bytes: None,
    };
    assert_eq!(decode::<SvgSource>(&encode(&held)).unwrap(), held);
}

#[test]
fn an_element_id_crosses_as_bin() {
    crosses_as_bin(ElementIdWire::Path(mixed()), &mixed());
    crosses_as_bin(ElementIdAtom::Path(mixed()), &mixed());
    crosses_as_bin(ElementIdWire::Uuid([0xc8; 16]), &[0xc8; 16]);
    crosses_as_bin(ElementIdAtom::Uuid([0xc8; 16]), &[0xc8; 16]);
    crosses_as_bin(ElementIdWire::OpaqueId([0xc8; 20]), &[0xc8; 20]);
    crosses_as_bin(
        ElementIdWire::NamedChild {
            base: ElementIdAtom::OpaqueId([0xc8; 20]),
            names: vec!["row".into()],
        },
        &[0xc8; 20],
    );
    // {3: bin} of the wrong length for what it names.
    let error = decode::<ElementIdWire>(&[0x81, 0x03, 0xc4, 0x02, 9, 9]).unwrap_err();
    assert!(error.contains("invalid length 2"), "{error}");
}

/// The one shape a byte field reads is `bin`: the array of integers it
/// crossed as before is refused, not read an element at a time.
#[test]
fn an_array_of_integers_is_not_bytes() {
    // {0: [1, 2]} where `ImageData::Encoded` holds a `bin`.
    let error = decode::<ImageData>(&[0x81, 0x00, 0x92, 0x01, 0x02]).unwrap_err();
    assert!(error.contains("expected bytes"), "{error}");
}

/// A picture's bound is checked on the `bin`'s length, and a `bin` that
/// says it is longer than the bytes it came in is refused where it stands:
/// neither is copied anywhere first.
#[test]
fn a_bin_past_its_bound_is_refused() {
    let error = decode::<ImageData>(&encode(&ImageData::Encoded(vec![0; MAX_FRAME_BYTES + 1])))
        .unwrap_err();
    assert!(error.contains("raster byte limit"), "{error}");
    let fits = encode(&ImageData::Encoded(vec![0; MAX_FRAME_BYTES]));
    assert_eq!(
        decode::<ImageData>(&fits).unwrap().byte_len(),
        MAX_FRAME_BYTES
    );

    // {0: bin32 of 4 GiB}, with nothing after the header.
    let start = std::time::Instant::now();
    assert!(decode::<ImageData>(&[0x81, 0x00, 0xc6, 0xff, 0xff, 0xff, 0xff]).is_err());
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}

/// One value is the whole of what was sent: anything after it refuses it.
#[test]
fn bytes_after_the_value_are_refused() {
    let mut wire = encode(&Event::Resync);
    assert_eq!(decode::<Event>(&wire).unwrap(), Event::Resync);
    wire.push(0xc0);
    assert_eq!(
        decode::<Event>(&wire).unwrap_err(),
        "trailing MessagePack bytes"
    );
}
