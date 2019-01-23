use crate::error::CryptoError;
use ring::aead::{self, Nonce, Aad};
use crate::localstd::borrow::ToOwned;
use crate::localstd::option::Option;
use crate::localstd::string::ToString;
use crate::localstd::vec::Vec;
use crate::localstd::vec;
use crate::rand;

const IV_SIZE: usize = 96/8;
static AES_MODE: &aead::Algorithm = &aead::AES_256_GCM;
type Key = [u8; 32];
type IV = [u8; IV_SIZE];


pub fn encrypt(message: &[u8], key: &Key) -> Result<Vec<u8>, CryptoError> { encrypt_with_nonce(message, key, None) }

pub fn encrypt_with_nonce(message: &[u8], key: &Key, _iv: Option<IV>) -> Result<Vec<u8>, CryptoError> {
    let iv = match _iv {
        Some(x) => x,
        None => {
            let mut _tmp_iv = [0; 12];
            rand::random(&mut _tmp_iv)?;
            _tmp_iv
        }
    };
    let aes_encrypt = aead::SealingKey::new(&AES_MODE, key)
        .map_err(|_| CryptoError::KeyError{ key_type: "Encryption".to_string(), err: Default::default() })?;

    let mut in_out = message.to_owned();
    let tag_size = AES_MODE.tag_len();
    in_out.extend(vec![0u8; tag_size]);
    let seal_size = {
        let iv = Nonce::assume_unique_for_key(iv);
        aead::seal_in_place(&aes_encrypt, iv, Aad::empty(), &mut in_out, tag_size)
            .map_err(|_| CryptoError::EncryptionError)
    }?;

    in_out.truncate(seal_size);
    in_out.extend_from_slice(&iv);
    Ok(in_out)
}


pub fn decrypt(cipheriv: &[u8], key: &Key) -> Result<Vec<u8>, CryptoError> {
    if cipheriv.len() < IV_SIZE {
        return Err(CryptoError::ImproperEncryption);
    }
    let aes_decrypt = aead::OpeningKey::new(&AES_MODE, key)
        .map_err(|_| CryptoError::KeyError { key_type: "Decryption".to_string(), err: Default::default() })?;

    let (ciphertext, iv) = cipheriv.split_at(cipheriv.len()-12);
    let nonce = aead::Nonce::try_assume_unique_for_key(&iv).unwrap(); // This Cannot fail because split_at promises that iv.len()==12
    let mut ciphertext = ciphertext.to_owned();

    let decrypted_data = aead::open_in_place(&aes_decrypt, nonce, Aad::empty(), 0, &mut ciphertext)
        .map_err(|_| CryptoError::DecryptionError)?;

    Ok(decrypted_data.to_vec())
}

#[cfg(test)]
pub mod tests {
    use crate::rand;
    use rustc_hex::{ToHex, FromHex};
    use crate::hash::Sha256;
    use super::{decrypt, encrypt_with_nonce};

    #[test]
    fn test_rand_encrypt_decrypt() {
        let mut rand_seed: [u8; 1072] = [0; 1072];
        rand::random(&mut rand_seed).unwrap();
        let mut key = [0u8; 32];
        key.copy_from_slice(&rand_seed[..32]);
        let mut iv: [u8; 12] = [0; 12];
        iv.clone_from_slice(&rand_seed[32..44]);
        let msg = rand_seed[44..1068].to_vec();
        let ciphertext = encrypt_with_nonce(&msg, &key, Some(iv)).unwrap();
        assert_eq!(msg, decrypt(&ciphertext, &key).unwrap());
    }

    #[test]
    fn test_encryption() {
        let key = b"EnigmaMPC".sha256();
        let msg = b"This Is Enigma".to_vec();
        let iv = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
        let result = encrypt_with_nonce(&msg, &key, Some(iv)).unwrap();
        assert_eq!(result.to_hex::<String>(), "02dc75395859faa78a598e11945c7165db9a16d16ada1b026c9434b134ae000102030405060708090a0b");
    }

    #[test]
    fn test_decryption() {
        let encrypted_data: Vec<u8> = "02dc75395859faa78a598e11945c7165db9a16d16ada1b026c9434b134ae000102030405060708090a0b".from_hex().unwrap();
        println!("{}", encrypted_data.len());
        let key = b"EnigmaMPC".sha256();
        let result = decrypt(&encrypted_data, &key).unwrap();
        assert_eq!(result, b"This Is Enigma".to_vec());

//        // for encryption purposes:
//        // use ethabi-cli to encode params then put the result in msg and get the encrypted arguments.
//        let key = get_key();
//        let iv = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
//        let msg = "0000000000000000000000005ed8cee6b63b1c6afce3ad7c92f4fd7e1b8fad9f".from_hex().unwrap();
//        let enc = encrypt_with_nonce(&msg, &key, Some(iv)).unwrap();

    }
}
