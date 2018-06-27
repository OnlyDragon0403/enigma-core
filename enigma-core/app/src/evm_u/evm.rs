#![allow(dead_code,unused_assignments,unused_variables)]
extern crate sgx_types;
extern crate sgx_urts;

use sgx_types::*;

use std::iter::FromIterator;
//failure 
use failure::Error;
use hex::ToHex;

extern {
    fn ecall_evm(eid: sgx_enclave_id_t,
                 retval: *mut sgx_status_t,
                 bytecode: *const u8, bytecode_len: usize,
                 callable: *const u8, callable_len: usize,
                 callable_args: *const u8, callable_args_len: usize,
                 preprocessor: *const u8, preprocessor_len: usize,
                 callback: *const u8, callback_len: usize,
                 output: *mut u8, signature: &mut [u8; 64],
                 result_length: &mut usize) -> sgx_status_t;
}


pub struct EvmInput {
    code: String,
    data: String,
}

// this is the input after its being parsed from the server (originally came from surface)

pub struct EvmRequest{
    #[allow(dead_code)]
    bytecode :      String,
    callable :      String,
    callable_args :  String,
    pub preprocessor :  Vec<String>,
    callback :      String,
}


impl EvmRequest {
     pub fn new(_bytecode:String,_callable:String,_callable_args:String,_preprocessor:Vec<String>,_callback:String) -> Self {
        EvmRequest {
            bytecode : _bytecode,
            callable : _callable, 
            callable_args : _callable_args, 
            preprocessor : _preprocessor,
            callback : _callback,
        }
    }
}   

// this is the result from the evm computation that will be send to the server and propagated to surface. 
#[derive(Serialize, Deserialize, Debug)]
pub struct EvmResponse{
    errored : bool,
    result : String,
    signature : String,
}


// this function is called by the the server componenet upon an execevm command from surface
// very likely that this functions will require an SgxEnclave object.

// TODO:: handle error and failure correctly with the 'result' variable returned from the enclave
// This should be changed
// the length of the result returned by EVM should be checked in advance
const MAX_EVM_RESULT: usize = 100000;
pub fn exec_evm(eid: sgx_enclave_id_t, evm_input: EvmRequest )-> Result<EvmResponse,Error>{
    let mut out = vec![0u8; MAX_EVM_RESULT];
    let slice = out.as_mut_slice();
    let mut signature: [u8; 64] = [0; 64];
    let mut retval: sgx_status_t = sgx_status_t::SGX_SUCCESS;
    let mut result_length: usize = 0;

    let mut prep: String = "".to_owned();
    for item in evm_input.preprocessor{
        prep.push_str(&item);
        prep.push(',');
    }
    prep.pop();

    let result = unsafe {
        ecall_evm(eid,
                  &mut retval,
                  evm_input.bytecode.as_ptr() as *const u8,
                  evm_input.bytecode.len(),
                  evm_input.callable.as_ptr() as *const u8,
                  evm_input.callable.len(),
                  evm_input.callable_args.as_ptr(),
                  evm_input.callable_args.len(),
                  //evm_input.preprocessor.as_ptr(),
                  prep.as_ptr(),
                  //evm_input.preprocessor.len(),
                  prep.len(),
                  evm_input.callback.as_ptr(),
                  evm_input.callback.len(),
                  slice.as_mut_ptr() as *mut u8,
                  &mut signature,
                  &mut result_length)
    };
    let part = Vec::from_iter(slice[0..result_length].iter().cloned());
    Ok(EvmResponse{
        errored: retval != sgx_status_t::SGX_SUCCESS,
        result: part.to_hex(),
        signature: signature.to_hex(),
    })
}

#[cfg(test)]
pub mod tests {
    #![allow(dead_code,unused_assignments,unused_variables)]
    use esgx;
    use std::fs::File;
    use std::io::{ BufReader, BufRead};
    use evm_u::evm;
    use super::{EvmRequest,EvmInput};
    use sgx_urts::SgxEnclave;

    fn read_input_from_file(path: &str) -> evm::EvmInput {
        let file = match File::open(&path) {
            // The `description` method of `io::Error` returns a string that
            // describes the error
            Err(why) => panic!("couldn't open {}: {}", path,
                               why),
            Ok(file) => file,
        };

        let mut lines = BufReader::new(file).lines();
        let result = evm::EvmInput {
            data: lines.next().unwrap().unwrap(),
            code: lines.next().unwrap().unwrap(),
        };
        result
    }

    fn init_enclave() -> SgxEnclave{
        let enclave = match esgx::general::init_enclave() {
            Ok(r) => {
                println!("[+] Init Enclave Successful {}!", r.geteid());
                r
            }
            Err(x) => {
                panic!("[-] Init Enclave Failed {}!", x.as_str());
            }
        };
        enclave
    }

    #[test]
    pub fn addition() {
        let evm_input = EvmRequest {
            bytecode:"606060405260043610603f576000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff168063ef9fc50b146044575b600080fd5b3415604e57600080fd5b606b60048080359060200190919080359060200190919050506081565b6040518082815260200191505060405180910390f35b600080828401905080915050929150505600a165627a7a72305820be9168caee2bd3045c4563ce44f698916986a5ad7b2148f91a35093d31d7211b0029".to_string(),
            callable: "addNumbers(uint,uint)".to_string(),
            callable_args: "f878b83a36373031663638663939343534623433633734373566616534613265613862376630303030313032303330343035303630373038303930613062b83a36343833333235643331323733613333633865626137353236646365666561636337303030313032303330343035303630373038303930613062".to_string(),
            preprocessor: [].to_vec(),
            callback : "".to_string(),
        };
        let enclave = init_enclave();
        let evm_result = match evm::exec_evm(enclave.geteid(), evm_input){
            Ok(v) => v,
            Err(e) => {
                println!("{}", e.to_string());
                return
            }
        };
        assert_eq!(evm_result.errored, false);
        assert_eq!(evm_result.result, "0000000000000000000000000000000000000000000000000000000000000003");
        enclave.destroy();
    }

    #[test]
    pub fn mix_addresses() {
        let evm_input = EvmRequest {
            bytecode: "6080604052600436106100c5576000357c0100000000000000000000000000000000000000000000000000000000900463ffffffff16806303988f84146100ca57806310f11e84146101705780632aaf281b1461026c5780633b833245146102fa57806357f5fc28146103415780636db0c8f0146103f7578063850d86191461049357806385e3c463146104b9578063a06a585614610529578063b24fd5c514610590578063daefe738146105e7578063dd20866e14610659578063ed0b494c14610735575b600080fd5b3480156100d657600080fd5b506100f56004803603810190808035906020019092919050505061080a565b604051808973ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200188600019166000191681526020018781526020018681526020018581526020018481526020018381526020018281526020019850505050505050505060405180910390f35b34801561017c57600080fd5b50610185610881565b60405180806020018060200180602001848103845287818151815260200191508051906020019060200280838360005b838110156101d05780820151818401526020810190506101b5565b50505050905001848103835286818151815260200191508051906020019060200280838360005b838110156102125780820151818401526020810190506101f7565b50505050905001848103825285818151815260200191508051906020019060200280838360005b83811015610254578082015181840152602081019050610239565b50505050905001965050505050505060405180910390f35b6102d6600480360381019080803563ffffffff169060200190929190803590602001908201803590602001908080601f0160208091040260200160405190810160405280939291908181526020018383808284378201915050505050509192919290505050610ab7565b604051808260018111156102e657fe5b60ff16815260200191505060405180910390f35b34801561030657600080fd5b5061032b600480360381019080803563ffffffff169060200190929190505050610f82565b6040518082815260200191505060405180910390f35b34801561034d57600080fd5b5061037c600480360381019080803563ffffffff16906020019092919080359060200190929190505050610fb5565b6040518080602001828103825283818151815260200191508051906020019080838360005b838110156103bc5780820151818401526020810190506103a1565b50505050905090810190601f1680156103e95780820380516001836020036101000a031916815260200191505b509250505060405180910390f35b34801561040357600080fd5b5061047d6004803603810190808035906020019082018035906020019080806020026020016040519081016040528093929190818152602001838360200280828437820191505050505050919291929080356000191690602001909291908035906020019092919080359060200190929190505050611095565b6040518082815260200191505060405180910390f35b6104b7600480360381019080803563ffffffff169060200190929190505050611108565b005b3480156104c557600080fd5b50610527600480360381019080803590602001909291908035906020019082018035906020019080806020026020016040519081016040528093929190818152602001838360200280828437820191505050505050919291929050505061122c565b005b34801561053557600080fd5b5061056c60048036038101908080356000191690602001909291908035906020019092919080359060200190929190505050611230565b6040518082600181111561057c57fe5b60ff16815260200191505060405180910390f35b34801561059c57600080fd5b506105a5611589565b604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390f35b3480156105f357600080fd5b50610618600480360381019080803563ffffffff1690602001909291905050506115ae565b604051808760001916600019168152602001868152602001858152602001848152602001838152602001828152602001965050505050505060405180910390f35b34801561066557600080fd5b506106d7600480360381019080803563ffffffff16906020019092919080359060200190820180359060200190808060200260200160405190810160405280939291908181526020018383602002808284378201915050505050509192919290803590602001909291905050506116da565b6040518083815260200180602001828103825283818151815260200191508051906020019060200280838360005b83811015610720578082015181840152602081019050610705565b50505050905001935050505060405180910390f35b34801561074157600080fd5b506107f46004803603810190808035906020019082018035906020019080806020026020016040519081016040528093929190818152602001838360200280828437820191505050505050919291929080356000191690602001909291908035906020019092919080359060200190820180359060200190808060200260200160405190810160405280939291908181526020018383602002808284378201915050505050509192919290505050611877565b6040518082815260200191505060405180910390f35b60018181548110151561081957fe5b90600052602060002090600b02016000915090508060000160009054906101000a900473ffffffffffffffffffffffffffffffffffffffff169080600101549080600301549080600401549080600501549080600601549080600701549080600a0154905088565b60608060608060608060006001805490506040519080825280602002602001820160405280156108c05781602001602082028038833980820191505090505b5093506001805490506040519080825280602002602001820160405280156108f75781602001602082028038833980820191505090505b50925060018054905060405190808252806020026020018201604052801561092e5781602001602082028038833980820191505090505b509150600090505b600180549050811015610aa55760018181548110151561095257fe5b90600052602060002090600b0201600a0154848281518110151561097257fe5b9060200190602002018181525050600060018281548110151561099157fe5b90600052602060002090600b020160020160003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020541115610a0557600183828151811015156109f657fe5b90602001906020020181815250505b3373ffffffffffffffffffffffffffffffffffffffff16600182815481101515610a2b57fe5b90600052602060002090600b020160000160009054906101000a900473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff161415610a985760018282815181101515610a8957fe5b90602001906020020181815250505b8080600101915050610936565b83838396509650965050505050909192565b600080600034111515610b32576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601f8152602001807f4465706f7369742076616c7565206d75737420626520706f7369746976652e0081525060200191505060405180910390fd5b600060018563ffffffff16815481101515610b4957fe5b90600052602060002090600b0201600a0154141515610bd0576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f496c6c6567616c20737461746520666f72206465706f736974732e000000000081525060200191505060405180910390fd5b60018463ffffffff16815481101515610be557fe5b90600052602060002090600b020190506000816006015434811515610c0657fe5b06141515610ca2576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252602f8152602001807f4465706f7369742076616c7565206d7573742062652061206d756c7469706c6581526020017f206f6620636c61696d2076616c7565000000000000000000000000000000000081525060400191505060405180910390fd5b60008160020160003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054141515610d81576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252602a8152602001807f43616e6e6f74206465706f73697420747769636520776974682074686520736181526020017f6d6520616464726573730000000000000000000000000000000000000000000081525060400191505060405180910390fd5b348160030160008282540192505081905550348160020160003373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000208190555082816008018260040154815481101515610def57fe5b906000526020600020019080519060200190610e0c9291906119bf565b50600181600401600082825401925050819055508363ffffffff163373ffffffffffffffffffffffffffffffffffffffff167fce7036acc3606aaa1ec3a2e7b4d13b3f4da34ee1eac298fc47524074de74a3bf8534600160405180806020018481526020018315151515815260200180602001838103835286818151815260200191508051906020019080838360005b83811015610eb7578082015181840152602081019050610e9c565b50505050905090810190601f168015610ee45780820380516001836020036101000a031916815260200191505b50838103825260088152602001807f616c6c20676f6f640000000000000000000000000000000000000000000000008152506020019550505050505060405180910390a380600701548160040154101515610f7757600181600a01819055508363ffffffff167fa98c11bc69afe22b520fe800f82e421f9594d4f06259a7600711b75af05a43b960405160405180910390a25b600091505092915050565b600060018263ffffffff16815481101515610f9957fe5b90600052602060002090600b0201600801805490509050919050565b606060018363ffffffff16815481101515610fcc57fe5b90600052602060002090600b020160080182815481101515610fea57fe5b906000526020600020018054600181600116156101000203166002900480601f0160208091040260200160405190810160405280929190818152602001828054600181600116156101000203166002900480156110885780601f1061105d57610100808354040283529160200191611088565b820191906000526020600020905b81548152906001019060200180831161106b57829003601f168201915b5050505050905092915050565b60008385848151811015156110a657fe5b90602001906020020190600019169081600019168152505082806001019350506110cf82611916565b85848151811015156110dd57fe5b9060200190602002019060001916908160001916815250508280600101935050829050949350505050565b600060608060018463ffffffff1681548110151561112257fe5b90600052602060002090600b02019250604080519080825280601f01601f1916602001820160405280156111655781602001602082028038833980820191505090505b50915060016040519080825280602002602001820160405280156111985781602001602082028038833980820191505090505b5090507f72616e64282900000000000000000000000000000000000000000000000000008160008151811015156111cb57fe5b9060200190602002019060001916908160001916815250508363ffffffff167fb37f76c8ba24e6a6d20d203681329001f2cacd9ab37c09d8b2aee57b8a31b8746001604051808215151515815260200191505060405180910390a250505050565b5050565b60008060018054905090506001805480919060010161124f9190611a3f565b503360018263ffffffff1681548110151561126657fe5b90600052602060002090600b020160000160006101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff1602179055508460018263ffffffff168154811015156112cb57fe5b90600052602060002090600b02016001018160001916905550600060018263ffffffff168154811015156112fb57fe5b90600052602060002090600b020160030181905550600060018263ffffffff1681548110151561132757fe5b90600052602060002090600b0201600401819055504260018263ffffffff1681548110151561135257fe5b90600052602060002090600b0201600501819055508360018263ffffffff1681548110151561137d57fe5b90600052602060002090600b0201600601819055508260018263ffffffff168154811015156113a857fe5b90600052602060002090600b020160070181905550826040519080825280602002602001820160405280156113f157816020015b60608152602001906001900390816113dc5790505b5060018263ffffffff1681548110151561140757fe5b90600052602060002090600b0201600801908051906020019061142b929190611a71565b508260405190808252806020026020018201604052801561145b5781602001602082028038833980820191505090505b5060018263ffffffff1681548110151561147157fe5b90600052602060002090600b02016009019080519060200190611495929190611ad1565b50600060018263ffffffff168154811015156114ad57fe5b90600052602060002090600b0201600a01819055508063ffffffff163373ffffffffffffffffffffffffffffffffffffffff167f8c2ac5e09d37c38a96fb20791b6ed6f2ccaaaf26c4115680b9257504d32bcdc34288888860016040518086815260200185600019166000191681526020018481526020018381526020018215151515815260200180602001828103825260088152602001807f616c6c20676f6f64000000000000000000000000000000000000000000000000815250602001965050505050505060405180910390a360009150509392505050565b6000809054906101000a900473ffffffffffffffffffffffffffffffffffffffff1681565b60008060008060008060008060008060008060018d63ffffffff168154811015156115d557fe5b90600052602060002090600b020160010154955060018d63ffffffff168154811015156115fe57fe5b90600052602060002090600b020160070154945060018d63ffffffff1681548110151561162757fe5b90600052602060002090600b020160060154935060018d63ffffffff1681548110151561165057fe5b90600052602060002090600b020160040154925060018d63ffffffff1681548110151561167957fe5b90600052602060002090600b020160030154915060018d63ffffffff168154811015156116a257fe5b90600052602060002090600b02016009018054905090508585858585859b509b509b509b509b509b5050505050505091939550919395565b600060606000806000865192505b600083111561185d5782600187016040518082815260200191505060405180910390206001900481151561171857fe5b069150866001840381518110151561172c57fe5b9060200190602002015173ffffffffffffffffffffffffffffffffffffffff16878381518110151561175a57fe5b9060200190602002015173ffffffffffffffffffffffffffffffffffffffff1614151561184f57866001840381518110151561179257fe5b90602001906020020151905086828151811015156117ac57fe5b9060200190602002015187600185038151811015156117c757fe5b9060200190602002019073ffffffffffffffffffffffffffffffffffffffff16908173ffffffffffffffffffffffffffffffffffffffff168152505080878381518110151561181257fe5b9060200190602002019073ffffffffffffffffffffffffffffffffffffffff16908173ffffffffffffffffffffffffffffffffffffffff16815250505b8280600190039350506116e8565b87878163ffffffff16915094509450505050935093915050565b60008084868581518110151561188957fe5b9060200190602002019060001916908160001916815250508380600101945050600090505b825181101561190a5782818151811015156118c557fe5b9060200190602002015186858151811015156118dd57fe5b906020019060200201906000191690816000191681525050838060010194505080806001019150506118ae565b83915050949350505050565b600080821415611948577f300000000000000000000000000000000000000000000000000000000000000090506119b7565b5b60008211156119b657610100816001900481151561196357fe5b0460010290507f01000000000000000000000000000000000000000000000000000000000000006030600a8481151561199857fe5b06010260010281179050600a828115156119ae57fe5b049150611949565b5b809050919050565b828054600181600116156101000203166002900490600052602060002090601f016020900481019282601f10611a0057805160ff1916838001178555611a2e565b82800160010185558215611a2e579182015b82811115611a2d578251825591602001919060010190611a12565b5b509050611a3b9190611b5b565b5090565b815481835581811115611a6c57600b0281600b028360005260206000209182019101611a6b9190611b80565b5b505050565b828054828255906000526020600020908101928215611ac0579160200282015b82811115611abf578251829080519060200190611aaf929190611c1e565b5091602001919060010190611a91565b5b509050611acd9190611c9e565b5090565b828054828255906000526020600020908101928215611b4a579160200282015b82811115611b495782518260006101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff16021790555091602001919060010190611af1565b5b509050611b579190611cca565b5090565b611b7d91905b80821115611b79576000816000905550600101611b61565b5090565b90565b611c1b91905b80821115611c1757600080820160006101000a81549073ffffffffffffffffffffffffffffffffffffffff0219169055600182016000905560038201600090556004820160009055600582016000905560068201600090556007820160009055600882016000611bf69190611d0d565b600982016000611c069190611d2e565b600a82016000905550600b01611b86565b5090565b90565b828054600181600116156101000203166002900490600052602060002090601f016020900481019282601f10611c5f57805160ff1916838001178555611c8d565b82800160010185558215611c8d579182015b82811115611c8c578251825591602001919060010190611c71565b5b509050611c9a9190611b5b565b5090565b611cc791905b80821115611cc35760008181611cba9190611d4f565b50600101611ca4565b5090565b90565b611d0a91905b80821115611d0657600081816101000a81549073ffffffffffffffffffffffffffffffffffffffff021916905550600101611cd0565b5090565b90565b5080546000825590600052602060002090810190611d2b9190611c9e565b50565b5080546000825590600052602060002090810190611d4c9190611b5b565b50565b50805460018160011615610100020316600290046000825580601f10611d755750611d94565b601f016020900490600052602060002090810190611d939190611b5b565b5b505600a165627a7a7230582015e1ffcde24bd26665fce5d7ea291f46d78d6cb87bc9fcf054851313b919bbef0029".to_string(),
            callable: "mixAddresses(uint32,address[],uint)".to_string(),
            callable_args: //Temp value, includes preprocessor
 "f9011832f90114b88831336431326537323439323462626230383930366434333239633063663138343062663239373562306339313963656238643530653830333463383066303437663261303438623264323034666363643664333061346439396137653239386166386235303837326639663039633464303030313032303330343035303630373038303930613062b88836313837326637623464323162386533613935333835343263633061663564303539663663303561306665653961656666383732396232313138383166333434663261393466623661373030383062336632333437646233376432653236316231616365336333313938636135656163303030313032303330343035303630373038303930613062".to_string(),
            preprocessor: ["rand".to_string()].to_vec(),
            callback : "distribute(uint,address[])".to_string(),
        };
        let enclave = init_enclave();
        let evm_result = match evm::exec_evm(enclave.geteid(), evm_input){
            Ok(v) => v,
            Err(e) => {
                println!("{}", e.to_string());
                return
            }
        };
        assert_eq!(evm_result.errored, false);
        assert!((evm_result.result == "85e3c463000000000000000000000000000000000000000000000000000000000000005000000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000002000000000000000000000000ee281427f13de12d46f1b910bfbc6346d041009f0000000000000000000000007331511dfdb45760f210af747d1ab275d935e4e8")
        |
                    (evm_result.result == "85e3c4630000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000020000000000000000000000007331511dfdb45760f210af747d1ab275d935e4e8000000000000000000000000ee281427f13de12d46f1b910bfbc6346d041009f"));
        //TODO add assert about signature correctness
        enclave.destroy();
    }



}