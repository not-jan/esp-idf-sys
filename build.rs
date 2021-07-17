use std::convert::TryFrom;
use std::{env, path::PathBuf};

use anyhow::*;

use pio;
use pio::bindgen;
use pio::project;
use pio::cargo::build;

fn main() -> Result<()> {
    let pio_scons_vars = if let Some(pio_scons_vars) = project::SconsVariables::from_piofirst() {
        println!("cargo:info=PIO->Cargo build detected: generating bindings only");

        pio_scons_vars
    } else {
        let pio = pio::Pio::install_default()?;

        let resolution = pio::Resolver::new(pio.clone())
            .params(pio::ResolutionParams {
                platform: Some("espressif32".into()),
                frameworks: vec!["espidf".into()],
                target: Some(env::var("TARGET")?),
                ..Default::default()
            })
            .resolve(true)?;

        let mut builder = project::Builder::new(PathBuf::from(env::var("OUT_DIR")?).join("esp-idf"));

        builder
            .enable_scons_dump()
            .enable_c_entry_points()
            .options(build::env_options_iter("ESP_IDF_SYS_PIO_CONF")?)
            .files(build::tracked_globs_iter(PathBuf::from("."), &["patches/**"])?)
            .files(build::tracked_env_globs_iter("ESP_IDF_SYS_GLOB")?);

        #[cfg(feature = "espidf_master")]
        builder.platform_package("framework-espidf", "https://github.com/ivmarkov/esp-idf.git#master");

        #[cfg(not(feature = "espidf_master"))]
        builder
            .platform_package_patch(PathBuf::from("patches").join("pthread_destructor_fix.diff"), PathBuf::from("framework-espidf"))
            .platform_package_patch(PathBuf::from("patches").join("missing_xtensa_atomics_fix.diff"), PathBuf::from("framework-espidf"));

        let project_path = builder.generate(&resolution)?;

        pio.build(&project_path, env::var("PROFILE")? == "release")?;

        let pio_scons_vars = project::SconsVariables::from_dump(&project_path)?;

        build::LinkArgs::try_from(&pio_scons_vars)?.propagate(project_path, true, true);

        pio_scons_vars
    };

    // In case other SYS crates need to have access to the ESP-IDF C headers
    build::CInclArgs::try_from(&pio_scons_vars)?.propagate();

    let mcu = pio_scons_vars.mcu.as_str();

    // Output the exact ESP32 MCU, so that we and crates depending directly on us can branch using e.g. #[cfg(esp32xxx)]
    println!("cargo:rustc-cfg={}", mcu);
    println!("cargo:MCU={}", mcu);

    let header = PathBuf::from("src")
        .join("include")
        .join(if mcu == "esp8266" { "esp-8266-rtos-sdk" } else { "esp-idf" })
        .join("bindings.h");

    build::track(&header)?;

    bindgen::run(bindgen::Factory::from_scons_vars(&pio_scons_vars)?.builder()?
        .ctypes_prefix("c_types")
        .header(build::to_string(header)?)
        .blacklist_function("strtold")
        .blacklist_function("_strtold_r")
        .clang_args(if mcu == "esp32c3" { vec!["-target", "riscv32"] } else { vec![] }),
    )
}
