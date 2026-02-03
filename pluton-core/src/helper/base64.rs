pub fn to_base64(input: Vec<u8>) -> String {
    // Authentic Slop
    let mut current_string = String::new();

    let lookup_array = [
      'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H',
      'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P',
      'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X',
      'Y', 'Z', 'a', 'b', 'c', 'd', 'e', 'f',
      'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n',
      'o', 'p', 'q', 'r', 's', 't', 'u', 'v',
      'w', 'x', 'y', 'z', '0', '1', '2', '3',
      '4', '5', '6', '7', '8', '9', '+', '/'
    ];

    for chunk in input.chunks(3) {
        let mut push_u8 = |byte: u8| {
            current_string.push(lookup_array[byte as usize]);
        };

        match chunk.len() {
            3 => { // No remainders, normal
                push_u8(chunk[0] >> 2);
                push_u8(((chunk[0] & 0b00000011) << 4) | (chunk[1] >> 4));
                push_u8(((chunk[1] & 0b00001111) << 2) | ((chunk[2] & 0b11000000) >> 6));
                push_u8(chunk[2] & 0b00111111);
            }
            2 => { // 2 remainders, one trailing =
                push_u8(chunk[0] >> 2);
                push_u8(((chunk[0] & 0b00000011) << 4) | (chunk[1] >> 4));
                push_u8((chunk[1] & 0b00001111) << 2);

                current_string.push('=');
            }
            1 => { // 1 remainder, two trailing =
                push_u8(chunk[0] >> 2);
                push_u8((chunk[0] & 0b00000011) << 4);

                current_string.push_str("==");
            }
            _ => { } 
        }
    }

    current_string
}


pub fn from_base64(input: String) -> Vec<u8> {
    let mut current_vec = Vec::with_capacity(input.len());

    // https://en.wikipedia.org/wiki/Base64
    // These values are the difference between the location of the character on the unicode table
    // and the position on the base64 table.
    fn match_base64_value(c: u8) -> u8 {
        match c {
            b'A'..=b'Z' => { c - b'A' }
            b'a'..=b'z' => { c - 71 }
            b'0'..=b'9' => { c + 4 }
            b'+' => { 62 }
            b'/' => { 63 }
            _ => { 64 }
        }
    }

    for chunk in input.as_bytes().chunks(4) {
        for byte in chunk {
            if match_base64_value(*byte) == 64 && *byte != b'=' {
                return Vec::new();
            }
        }

        current_vec.push((match_base64_value(chunk[0]) << 2) | (match_base64_value(chunk[1]) >> 4));

        if chunk[2] != b'=' {
            current_vec.push(((match_base64_value(chunk[1]) & 0b00001111) << 4) | (match_base64_value(chunk[2]) >> 2));
        }
        if chunk[3] != b'=' {
            current_vec.push(((match_base64_value(chunk[2]) & 0b00000011) << 6) | (match_base64_value(chunk[3])));
        }
    }

    current_vec
}

pub fn to_base64url(input: Vec<u8>) -> String {
    let mut to_base64_normal = to_base64(input);

    to_base64_normal = to_base64_normal.replace("+", "-");
    to_base64_normal = to_base64_normal.replace("/", "_");
    to_base64_normal = to_base64_normal.replace("=", ".");

    to_base64_normal
}

pub fn from_base64url(input: String) -> Vec<u8> {
    let mut new_input = input;
    new_input = new_input.replace("-", "+");
    new_input = new_input.replace("_", "/");
    new_input = new_input.replace(".", "=");

    from_base64(new_input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_base64_basic_1() {
        let input = b"Many hands make light work.".to_vec();
        let output = to_base64(input);

        assert_eq!(output, "TWFueSBoYW5kcyBtYWtlIGxpZ2h0IHdvcmsu");
    }

    #[test]
    fn to_base64_basic_2() {
        let input = b"Man".to_vec();
        let output = to_base64(input);

        assert_eq!(output, "TWFu");
    }

    #[test]
    fn to_base64_rem_1() {
        let input = b"light work.".to_vec();
        let output = to_base64(input);

        assert_eq!(output, "bGlnaHQgd29yay4=");
    }

    #[test]
    fn to_base64_rem_2() {
        let input = b"light work".to_vec();
        let output = to_base64(input);

        assert_eq!(output, "bGlnaHQgd29yaw==");
    }

    #[test]
    fn from_base64_basic_1() {
        let input = "TWFueSBoYW5kcyBtYWtlIGxpZ2h0IHdvcmsu".to_string();
        let output = from_base64(input);

        assert_eq!(output, b"Many hands make light work.".to_vec());
    }

    #[test]
    fn from_base64_basic_2() {
        let input = "TWFu".to_string();
        let output = from_base64(input);

        assert_eq!(output, b"Man".to_vec());
    }

    #[test]
    fn from_base64_rem_1() {
        let input = "bGlnaHQgd29yay4=".to_string();
        let output = from_base64(input);

        assert_eq!(output, b"light work.".to_vec());
    }

    #[test]
    fn from_base64_rem_2() {
        let input = "bGlnaHQgd29yaw==".to_string();
        let output = from_base64(input);

        assert_eq!(output, b"light work".to_vec());
    }
}
