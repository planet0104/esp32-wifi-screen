use anyhow::Result;
use data_encoding::BASE64;
use std::net::Ipv4Addr;

pub fn is_same_subnet(ip1: Ipv4Addr, ip2: Ipv4Addr, subnet_mask: Ipv4Addr) -> bool {
    let network1 = u32::from(ip1) & u32::from(subnet_mask);
    let network2 = u32::from(ip2) & u32::from(subnet_mask);
    network1 == network2
}

pub fn decode_base64(input:&str) -> Result<Box<Vec<u8>>>{
    let input_byte = input.as_bytes();
    let mut output = Box::new(vec![0u8; BASE64.decode_len(input_byte.len())?]);
    let len = BASE64.decode_mut(input_byte, &mut output).map_err(|partial| partial.error)?;
    output.truncate(len);
    Ok(output)
}