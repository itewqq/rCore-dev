use std::fs::{File, read_dir};
use std::{hash::Hasher, io::Write};

static TARGET_PATH: &str = "../user/target/riscv64gc-unknown-none-elf/release/";

fn main(){
    let mut link_file = match File::open("./src/link_app.S") {
        Ok(f) => f,
        Err(_) => File::create("./src/link_app.S").unwrap()
    };
    let mut paths: Vec<_> = read_dir("../user/src/bin/")
        .unwrap()
        .into_iter()
        .map(|file_path| {
            let mut file_ext = file_path.unwrap().file_name().into_string().unwrap();
            // file_ext.split('/').collect::<Vec<&str>>().pop().unwrap().to_string();
            file_ext.drain(file_ext.len()-3..file_ext.len());// remove ".rs"
            file_ext
        })
        .collect();
    let app_number = paths.len();
    paths.sort();

    let app_info_list = format!(r#"
    .align 3
    .section .data
    .global _num_app
_num_app:
    .quad {}"#, app_number);
    link_file.write(app_info_list.as_bytes()).ok();

    for i in 0..app_number {
        let item = format!(r#"
    .quad app_{}_start"#, i);
        link_file.write(item.as_bytes()).ok();
    }

    let item_end = format!(r#"
    .quad app_{}_end
"#, app_number-1);
        link_file.write(item_end.as_bytes()).ok();

    for (i, path) in paths.iter().enumerate() {
        let bin_code = format!(r#"
    .section .data
    .global app_{0}_start
    .global app_{0}_end
app_{0}_start:
    .incbin "{1}{2}.bin"
app_{0}_end:
"#, i, TARGET_PATH, path);
        link_file.write(bin_code.as_bytes()).ok();
    }
}
