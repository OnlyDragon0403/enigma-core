use sgx_types::{sgx_attributes_t, sgx_launch_token_t, sgx_misc_attribute_t, SgxResult};
use sgx_urts::SgxEnclave;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{self, PathBuf, Path};
use dirs;
use failure::Error;

pub fn storage_dir<P: AsRef<Path>>(dir_name: P) -> Result<PathBuf, Error> {
    let mut path = dirs::home_dir().ok_or_else(|| format_err!("Missing HomeDir"))?;
    trace!("Home dir is {}", path.display());
    path.push(dir_name);
    Ok(path)
}

pub fn init_enclave(token_path: &path::PathBuf, use_token: bool, enclave_location: &str)
    -> SgxResult<(SgxEnclave, Option<sgx_launch_token_t>)> {
    let path = env::current_dir().unwrap();
    trace!("The current directory is {}", path.display());
    let mut launch_token: sgx_launch_token_t = [0; 1024];
    let mut launch_token_updated: i32 = 0;

    if use_token {
        match fs::File::open(&token_path) {
            Err(_) => {
                error!("Open token file {} error! Will create one.", token_path.as_path().to_str().unwrap());
            }
            Ok(mut f) => {
                info!("Open token file success! ");
                match f.read(&mut launch_token) {
                    Ok(1024) => {
                        info!("Token file valid!");
                    }
                    _ => warn!("Token file invalid, will create new token file"),
                }
            }
        }
    }

    // Step 2: call sgx_create_enclave to initialize an enclave instance
    // Debug Support: set 2nd parameter to 1
    let debug = 1;
    let mut misc_attr = sgx_misc_attribute_t { secs_attr: sgx_attributes_t { flags: 0, xfrm: 0 }, misc_select: 0 };
    let enclave = SgxEnclave::create(enclave_location, debug, &mut launch_token, &mut launch_token_updated, &mut misc_attr)?;

    if launch_token_updated != 0 {
        info!("Enclave created, Token: {:?}", enclave);
        return Ok((enclave, Some(launch_token)));
    }
    Ok((enclave, None))
}
