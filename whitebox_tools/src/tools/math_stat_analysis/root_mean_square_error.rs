/* 
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: September 18, 2017
Last Modified: September 18, 2017
License: MIT
*/
extern crate time;
extern crate num_cpus;

use std::env;
use std::path;
use std::f64;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use raster::*;
use std::io::{Error, ErrorKind};
use tools::WhiteboxTool;

pub struct RootMeanSquareError {
    name: String,
    description: String,
    parameters: String,
    example_usage: String,
}

impl RootMeanSquareError {
    pub fn new() -> RootMeanSquareError { // public constructor
        let name = "RootMeanSquareError".to_string();
        
        let description = "Calculates the RMSE and other accuracy statistics.".to_string();
        
        let mut parameters = "-i, --input    Input raster file.".to_owned();
        parameters.push_str("--base     Base raster used for comparison.\n");
         
        let sep: String = path::MAIN_SEPARATOR.to_string();
        let p = format!("{}", env::current_dir().unwrap().display());
        let e = format!("{}", env::current_exe().unwrap().display());
        let mut short_exe = e.replace(&p, "").replace(".exe", "").replace(".", "").replace(&sep, "");
        if e.contains(".exe") {
            short_exe += ".exe";
        }
        let usage = format!(">>.*{0} -r={1} --wd=\"*path*to*data*\" -i=DEM.dep", short_exe, name).replace("*", &sep);
    
        RootMeanSquareError { name: name, description: description, parameters: parameters, example_usage: usage }
    }
}

impl WhiteboxTool for RootMeanSquareError {
    fn get_tool_name(&self) -> String {
        self.name.clone()
    }

    fn get_tool_description(&self) -> String {
        self.description.clone()
    }

    fn get_tool_parameters(&self) -> String {
        self.parameters.clone()
    }

    fn get_example_usage(&self) -> String {
        self.example_usage.clone()
    }

    fn run<'a>(&self, args: Vec<String>, working_directory: &'a str, verbose: bool) -> Result<(), Error> {
        let mut input_file = String::new();
        let mut base_file = String::new();
         
        if args.len() == 0 {
            return Err(Error::new(ErrorKind::InvalidInput,
                                "Tool run with no paramters. Please see help (-h) for parameter descriptions."));
        }
        for i in 0..args.len() {
            let mut arg = args[i].replace("\"", "");
            arg = arg.replace("\'", "");
            let cmd = arg.split("="); // in case an equals sign was used
            let vec = cmd.collect::<Vec<&str>>();
            let mut keyval = false;
            if vec.len() > 1 {
                keyval = true;
            }
            if vec[0].to_lowercase() == "-i" || vec[0].to_lowercase() == "--input" {
                if keyval {
                    input_file = vec[1].to_string();
                } else {
                    input_file = args[i+1].to_string();
                }
            } else if vec[0].to_lowercase() == "-base" || vec[0].to_lowercase() == "--base" {
                if keyval {
                    base_file = vec[1].to_string();
                } else {
                    base_file = args[i+1].to_string();
                }
            }
        }

        if verbose {
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
            println!("* Welcome to {} *", self.get_tool_name());
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
        }

        let sep: String = path::MAIN_SEPARATOR.to_string();

        let mut progress: usize;
        let mut old_progress: usize = 1;

        if !input_file.contains(&sep) {
            input_file = format!("{}{}", working_directory, input_file);
        }
        if !base_file.contains(&sep) {
            base_file = format!("{}{}", working_directory, base_file);
        }

        if verbose { println!("Reading data...") };

        let input = Arc::new(Raster::new(&input_file, "r")?);
        let base_raster = Arc::new(Raster::new(&base_file, "r")?);

        let start = time::now();
        let rows = input.configs.rows as isize;
        let columns = input.configs.columns as isize;
        let nodata = input.configs.nodata;
        let nodata_base = base_raster.configs.nodata;

        if base_raster.configs.rows as isize == rows && base_raster.configs.columns as isize == columns {
            // The two grids are the same resolution. This simplifies calculation greatly.
            let num_procs = num_cpus::get() as isize;
            let (tx, rx) = mpsc::channel();
            for tid in 0..num_procs {
                let input = input.clone();
                let base_raster = base_raster.clone();
                let tx = tx.clone();
                thread::spawn(move || {
                    let mut z1: f64;
                    let mut z2: f64;
                    for row in (0..rows).filter(|r| r % num_procs == tid) {
                        let mut n = 0i32;
                        let mut s = 0.0f64;
                        let mut sq = 0.0f64;
                        for col in 0..columns {
                            z1 = input[(row, col)];
                            z2 = base_raster[(row, col)];
                            if z1 != nodata && z2 != nodata_base {
                                n += 1;
                                s += z1 - z2;
                                sq += (z1 - z2) * (z1 - z2);
                            }
                        }
                        tx.send((n, s, sq)).unwrap();
                    }
                });
            }

            let mut num_cells = 0i32;
            let mut sum = 0.0;
            let mut sq_sum = 0.0;
            for row in 0..rows {
                let (a, b, c) = rx.recv().unwrap();
                num_cells += a;
                sum += b;
                sq_sum += c;

                if verbose {
                    progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            let rmse = (sq_sum / num_cells as f64).sqrt();
			let mean_vertical_error = sum / num_cells as f64;

            println!("\nVertical Accuracy Analysis:\n");
			println!("Comparison File: {}", input_file);
			println!("Base File: {}", base_file);
			println!("Mean vertical error: {:.4}", mean_vertical_error); 
			println!("RMSE: {:.4}", rmse);
			println!("Accuracy at 95% confidence limit (m): {:.4}", rmse * 1.96f64);

        } else {
            /* The two grids are not of the same resolution. Bilinear resampling will have to be 
                carried out to estimate z-values. Base image = source; input image = destination */
            let num_procs = num_cpus::get() as isize;
            let (tx, rx) = mpsc::channel();
            for tid in 0..num_procs {
                let input = input.clone();
                let base_raster = base_raster.clone();
                let tx = tx.clone();
                thread::spawn(move || {
                    let mut y: f64;
                    let mut x: f64;
                    let mut z1: f64;
                    let mut z2: f64;
                    let mut src_row: f64;
                    let mut src_col: f64;
                    let mut origin_row: isize;
                    let mut origin_col: isize;
                    let mut dx: f64;
                    let mut dy: f64;
                    let src_north = base_raster.configs.north;
                    let src_east = base_raster.configs.east;
                    let src_resolution_x = base_raster.configs.resolution_x;
                    let src_resolution_y = base_raster.configs.resolution_y;
                    let mut n0: f64;
                    let mut n1: f64;
                    let mut n2: f64;
                    let mut n3: f64;
                    
                    for row in (0..rows).filter(|r| r % num_procs == tid) {
                        y = input.get_y_from_row(row);
                        let mut n = 0i32;
                        let mut s = 0.0f64;
                        let mut sq = 0.0f64;
                        for col in 0..columns {
                            z1 = input[(row, col)];
                            if z1 != nodata {
                                x = input.get_x_from_column(col);
                                src_row = (src_north - y) / src_resolution_y;
                                src_col = (x - src_east) / src_resolution_x;
                                origin_row = src_row.floor() as isize;
                                origin_col = src_col.floor() as isize;
                                dx = src_col - src_col.floor();
                                dy = src_row - src_row.floor();

                                n0 = base_raster[(origin_row, origin_col)];
                                n1 = base_raster[(origin_row, origin_col + 1)];
                                n2 = base_raster[(origin_row + 1, origin_col)];
                                n3 = base_raster[(origin_row + 1, origin_col+ 1)];

                                if n0 != nodata_base && n1 != nodata_base && n2 != nodata_base && n3 != nodata_base { 
                                    // This is the bilinear interpolation equation.
                                    z2 = n0 * (1f64 - dx) * (1f64 - dy) + n1 * dx * (1f64 - dy) + n2 * (1f64 - dx) * dy + n3 * dx * dy;
                                } else {
                                    // some of the neighbours are nodata and an inverse-distance scheme is used instead
                                    let w0 = if n0 != nodata_base { 1f64 / (dx * dx + dy * dy) } else { 0f64 };
                                    let w1 = if n1 != nodata_base { 1f64 / ((1f64-dx) * (1f64-dx) + dy * dy) } else { 0f64 };
                                    let w2 = if n2  != nodata_base { 1f64 / (dx * dx + (1f64-dy) * (1f64-dy)) } else { 0f64 };
                                    let w3 = if n3 != nodata_base { 1f64 / ((1f64-dx) * (1f64-dx) + (1f64-dy) * (1f64-dy)) } else { 0f64 };
                                    let sum = w0 + w1 + w2 + w3;
                                    if sum > 0f64 {
                                        z2 = (n0 * w0 + n1 * w1 + n2 * w2 + n3 * w3) / sum;
                                    } else {
                                        z2 = nodata_base;
                                    }
                                }

                                if z2 != nodata_base && !z2.is_nan() {
                                    n += 1;
                                    s += z1 - z2;
                                    sq += (z1 - z2) * (z1 - z2);
                                }
                            }
                        }
                        tx.send((n, s, sq)).unwrap();
                    }
                });
            }

            let mut num_cells = 0i32;
            let mut sum = 0.0;
            let mut sq_sum = 0.0;
            for row in 0..rows {
                let (a, b, c) = rx.recv().unwrap();
                num_cells += a;
                sum += b;
                sq_sum += c;

                if verbose {
                    progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                    if progress != old_progress {
                        println!("Progress: {}%", progress);
                        old_progress = progress;
                    }
                }
            }

            let rmse = (sq_sum / num_cells as f64).sqrt();
			let mean_vertical_error = sum / num_cells as f64;

            println!("\nVertical Accuracy Analysis:\n");
			println!("Comparison File: {}", input_file);
			println!("Base File: {}", base_file);
			println!("Mean vertical error: {:.4}", mean_vertical_error); 
			println!("RMSE: {:.4}", rmse);
			println!("Accuracy at 95% confidence limit (m): {:.4}", rmse * 1.96f64);

        }

        let end = time::now();
        let elapsed_time = end - start;

        println!("\n{}", &format!("Elapsed Time (excluding I/O): {}", elapsed_time).replace("PT", ""));

        Ok(())
    }
}