use aes::Aes256;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::NoPadding};
use hmac::{Hmac, Mac};
use sha2::Sha512;

const PAGE: usize = 4096;
const SALT: usize = 16;
const IV: usize = 16;
const MAC: usize = 64;
const RESERVE: usize = IV + MAC;
const KDF_ROUNDS: u32 = 256_000;
const MAC_ROUNDS: u32 = 2;
const MAC_SALT_MASK: u8 = 0x3a;
const HEADER: &[u8; SALT] = b"SQLite format 3\0";

type HmacSha512 = Hmac<Sha512>;

pub fn decrypt(data: &[u8], passphrase: &[u8]) -> crate::Result<Vec<u8>> {
    decrypt_with(data, passphrase, KDF_ROUNDS)
}

fn invalid(what: &str) -> crate::Error {
    crate::Error::Invalid(what.to_owned())
}

fn keys(passphrase: &[u8], salt: &[u8], rounds: u32) -> crate::Result<([u8; 32], [u8; 32])> {
    let mut key = [0; 32];
    pbkdf2::pbkdf2::<HmacSha512>(passphrase, salt, rounds, &mut key)
        .map_err(|_| invalid("cannot derive the database key"))?;
    let mac_salt: Vec<u8> = salt.iter().map(|b| b ^ MAC_SALT_MASK).collect();
    let mut mac = [0; 32];
    pbkdf2::pbkdf2::<HmacSha512>(&key, &mac_salt, MAC_ROUNDS, &mut mac)
        .map_err(|_| invalid("cannot derive the database key"))?;
    Ok((key, mac))
}

fn page_mac(mac_key: &[u8], signed: &[u8], number: u32) -> crate::Result<HmacSha512> {
    let mut mac = HmacSha512::new_from_slice(mac_key).map_err(|_| invalid("bad page key"))?;
    mac.update(signed);
    mac.update(&number.to_le_bytes());
    Ok(mac)
}

fn decrypt_with(data: &[u8], passphrase: &[u8], rounds: u32) -> crate::Result<Vec<u8>> {
    if data.starts_with(HEADER) {
        return Ok(data.to_vec());
    }
    if data.len() < PAGE || !data.len().is_multiple_of(PAGE) {
        return Err(invalid("not an encrypted library database"));
    }
    let (key, mac_key) = keys(passphrase, &data[..SALT], rounds)?;
    let mut out = Vec::with_capacity(data.len());
    for (i, page) in data.as_chunks::<PAGE>().0.iter().enumerate() {
        let number = u32::try_from(i + 1).map_err(|_| invalid("database too large"))?;
        let start = if i == 0 { SALT } else { 0 };
        page_mac(&mac_key, &page[start..PAGE - MAC], number)?
            .verify_slice(&page[PAGE - MAC..])
            .map_err(|_| invalid("the library database is damaged or uses another key"))?;
        let mut body = page[start..PAGE - RESERVE].to_vec();
        cbc::Decryptor::<Aes256>::new_from_slices(&key, &page[PAGE - RESERVE..PAGE - MAC])
            .map_err(|_| invalid("bad page key"))?
            .decrypt_padded_mut::<NoPadding>(&mut body)
            .map_err(|_| invalid("bad page size"))?;
        if i == 0 {
            out.extend_from_slice(HEADER);
        }
        out.extend_from_slice(&body);
        out.extend_from_slice(&page[PAGE - RESERVE..]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use cbc::cipher::BlockEncryptMut;

    use super::*;

    fn encrypt(plain: &[u8], passphrase: &[u8], salt: [u8; SALT], rounds: u32) -> Vec<u8> {
        let (key, mac_key) = keys(passphrase, &salt, rounds).unwrap();
        let mut out = Vec::new();
        for (i, page) in plain.as_chunks::<PAGE>().0.iter().enumerate() {
            let start = if i == 0 { SALT } else { 0 };
            let iv = [u8::try_from(i % 251).unwrap(); IV];
            let mut body = page[start..PAGE - RESERVE].to_vec();
            let len = body.len();
            cbc::Encryptor::<Aes256>::new_from_slices(&key, &iv)
                .unwrap()
                .encrypt_padded_mut::<NoPadding>(&mut body, len)
                .unwrap();
            let mut enc = if i == 0 { salt.to_vec() } else { Vec::new() };
            enc.extend(body);
            enc.extend(iv);
            let number = u32::try_from(i + 1).unwrap();
            let mac = page_mac(&mac_key, &enc[start..], number).unwrap();
            enc.extend(mac.finalize().into_bytes());
            out.extend(enc);
        }
        out
    }

    fn plain_pages(count: usize) -> Vec<u8> {
        let mut plain: Vec<u8> = (0..PAGE * count)
            .map(|i| u8::try_from(i % 253).unwrap())
            .collect();
        plain[..SALT].copy_from_slice(HEADER);
        for page in plain.as_chunks_mut::<PAGE>().0 {
            page[PAGE - RESERVE..].fill(0);
        }
        plain
    }

    #[test]
    fn pages_decrypt_back_to_the_plain_database() {
        let plain = plain_pages(3);
        let enc = encrypt(&plain, b"secret", [7; SALT], 4);
        assert_ne!(enc[..SALT], plain[..SALT]);
        let out = decrypt_with(&enc, b"secret", 4).unwrap();
        assert_eq!(out.len(), plain.len());
        for (o, p) in out
            .as_chunks::<PAGE>()
            .0
            .iter()
            .zip(plain.as_chunks::<PAGE>().0)
        {
            assert_eq!(o[..PAGE - RESERVE], p[..PAGE - RESERVE]);
        }
    }

    #[test]
    fn a_wrong_key_or_damaged_page_is_refused() {
        let enc = encrypt(&plain_pages(2), b"secret", [7; SALT], 4);
        assert!(decrypt_with(&enc, b"other", 4).is_err());
        let mut damaged = enc.clone();
        damaged[PAGE + 10] ^= 1;
        assert!(decrypt_with(&damaged, b"secret", 4).is_err());
        assert!(decrypt_with(&enc[..=PAGE], b"secret", 4).is_err());
    }

    #[test]
    fn a_plain_database_is_read_as_is() {
        let plain = plain_pages(1);
        assert_eq!(decrypt_with(&plain, b"secret", 4).unwrap(), plain);
    }
}
