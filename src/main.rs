extern crate glob;
use glob::glob;

extern crate aho_corasick;
use aho_corasick::AhoCorasick;

use std::io::prelude::*;
use std::fs::File;
use std::env;
use std::fs;

struct MatchedFile {
    // The absolute path of the matched file.
    file_path:String,

    // The list of patterns that matched.
    matched_patterns:Vec<String>
}

struct SkippedFile {
    // Absolute path of the file that was skipped, can be "Unknown".
    file_path:String,

    // The reason that the file was skipped.
    skip_reason:String
}

struct SearchResults {
    // Files that met the provided conditions, and matched one or more provided patterns.
    matched_files:Vec<MatchedFile>,

    // Files that were skipped for some reason.
    skipped_files:Vec<SkippedFile>,
    
    // Candidate files that met the provided conditions, but didn't match any of the provided patterns.
    unmatched_files:Vec<String>
}

fn perform_search(directory:&String, file_extensions:&Vec<String>, patterns:&Vec<String>, max_file_size:&u64, max_files:&usize) -> Result<SearchResults, String> {
    let mut search_results = SearchResults {
        matched_files:Vec::new(),
        skipped_files:Vec::new(),
        unmatched_files:Vec::new(),
    };

    let extensions_matter:bool = file_extensions.len() > 0;
    let file_size_matters:bool = *max_file_size > 0;
    let file_count_matters:bool = *max_files > 0;

    let glob_pattern:String = if directory.ends_with("/") || directory.ends_with("\\") { directory.clone() + "**/*" } else { directory.clone() + "/**/*" };

    let directory_entries = match glob(glob_pattern.as_str()) {
        Ok(directory_entries) => directory_entries,
        Err(error) => return Err(format!("Couldn't retrieve directory entries for the directory ({}), error: {:?}", directory, error))
    };

    // List of queued files that will be searched for matching patterns.
    let mut queued_files:Vec<String> = Vec::new();

    // Fill the queue with candidate files.
    for (index, element) in directory_entries.enumerate() {
        let path_obj = match element {
            Ok(file_path) => file_path,
            Err(error) => {
                let skipped_file = SkippedFile { 
                    file_path:String::from("Unknown"),
                    skip_reason:format!("Skipped due to error when matching element: {:?}", error)
                };

                search_results.skipped_files.push(skipped_file);
                continue;
            }
        };

        // If the path points to a file, continue.
        if path_obj.is_file() {
            let absolute_file_path:String = match path_obj.to_str() {
                Some(absolute_file_path) => String::from(absolute_file_path),
                None => {
                    let skipped_file = SkippedFile {
                        file_path:String::from("Unknown"),
                        skip_reason:format!("Couldn't convert the PathBuf into a string to get the absolute file path, presumably because the path is invalid UTF-8.")
                    };

                    search_results.skipped_files.push(skipped_file);
                    continue;
                }
            };

            let file_size:u64 = match fs::metadata(&path_obj) {
                Ok(file_metadata) => file_metadata.len(),
                Err(error) => {
                    let skipped_file = SkippedFile {
                        file_path:absolute_file_path,
                        skip_reason:format!("Error when retrieving the file's size: {:?}", error)
                    };

                    search_results.skipped_files.push(skipped_file);
                    continue;
                }
            };

            // If the amount of queued files exceeds the maximum, break and proceed with the search.
            if file_count_matters && queued_files.len() > *max_files {
                break;
            }

            if extensions_matter && !file_extensions.iter().any(|file_extension| absolute_file_path.ends_with(file_extension)) {
                let skipped_file = SkippedFile {
                    file_path:absolute_file_path,
                    skip_reason:format!("The file did not end with any of the provided extensions.")
                };

                search_results.skipped_files.push(skipped_file);
                continue;
            }

            // Proceed if the file size doesn't natter, or if it does matter and the file size is less than the provided maximum.
            if !file_size_matters || (file_size_matters && file_size <= *max_file_size) {
                queued_files.push(absolute_file_path);

            } else {
                let skipped_file = SkippedFile {
                    file_path:absolute_file_path,
                    skip_reason:format!("The file exceeded the provided size ({} > {})", file_size, max_file_size)
                };

                search_results.skipped_files.push(skipped_file);
                continue;
            }
        }
        
        print!("Queueing files.. {} / {} Files have been queued..\r", queued_files.len(), index + 1);
    }

    println!("");

    let mut last_message_size:usize = 0;

    for (index, queued_file) in queued_files.iter().enumerate() {
        let relative_file_path:String = match queued_file.clone().split("\\").last() {
            Some(relative_file_path) => String::from(relative_file_path),
            None => String::from(queued_file)
        };

        let mut message = format!("[{} / {}] Searching through {} for patterns..", index + 1, queued_files.len(), relative_file_path);

        if message.len() < last_message_size {
            message += " ".repeat(last_message_size - message.len()).as_str();
        }

        last_message_size = message.len();
        
        print!("{}\r", message);

        let mut file_stream = match File::open(&queued_file) {
            Ok(stream) => stream,
            Err(error) => {
                let skipped_file = SkippedFile {
                    file_path:queued_file.clone(),
                    skip_reason:format!("Failed to open stream to file @ {}, error: {:?}", queued_file, error)
                };

                search_results.skipped_files.push(skipped_file);
                continue;
            }
        };

        let mut file_contents:Vec<u8> = Vec::new();

        let _ = match file_stream.read_to_end(&mut file_contents) {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                let skipped_file = SkippedFile {
                    file_path:queued_file.clone(),
                    skip_reason:format!("Failed to read data from file @ {}, error: {:?}", queued_file, error)
                };

                search_results.skipped_files.push(skipped_file);
                continue;
            }
        };

        let aho_corasick_search_alg:AhoCorasick = AhoCorasick::new(patterns);

        let mut matched_patterns:Vec<String> = Vec::new();

        for matched_pattern in aho_corasick_search_alg.find_iter(&file_contents) {
            let pattern_as_string:&String = &patterns[matched_pattern.pattern()];

            if !matched_patterns.contains(pattern_as_string) {
                matched_patterns.push(pattern_as_string.clone());
            }
        }

        if matched_patterns.len() > 0 {
            let matched_file = MatchedFile {
                file_path:queued_file.clone(),
                matched_patterns:matched_patterns.clone()
            };

            search_results.matched_files.push(matched_file);
        } else {
            search_results.unmatched_files.push(queued_file.clone());
        }
    }

    println!();
    
    return Ok(search_results);
}

const HELP_MESSAGE:&str = "
-spt    | [Necessary] The pattern(s) used to match files. Every argument past this one is considered a pattern, and thus it must be placed after other arguments.
-dir    | Specifies the directory to perform the operation, if not specified blank, assumes working directory.
-mfs    | Do not queue files that exceed this size in bytes.
-mfq    | Maximum amount of queued files allowed.
-ssk    | Show files that were skipped, and the reason behind skipping them.
-sum    | Show unmatched files (files that met the queue conditions, but didn't match any given pattern).
-ext    | Only queue files with one of the provided extensions, e.g. .cpp:.hpp
-h      | Displays this help message.
";

fn main() {
    let mut target_directory:String         =       String::from(".");

    let mut file_extensions:Vec<String>     =       Vec::new();
    let mut search_patterns:Vec<String>     =       Vec::new();

    let mut maximum_file_size:u64           =       0;
    let mut maximum_files_queued:usize      =       0;

    let mut show_unmatched:bool             =       false;
    let mut show_skipped:bool               =       false;

    // Create a peekable iterator over the console arguments.
    let mut argument_iterator = env::args().peekable();

    // Parse arguments in argument iterator.
    loop {
        let argument = match argument_iterator.next() {
            Some(argument) => argument,
            None => break
        };

        let peek_result = argument_iterator.peek();

        let next_argument_present:bool = match peek_result {
            Some(_) => true,
            None => false
        };

        let next_argument:&String = match peek_result {
            Some(string) => string,
            None => &argument
        };

        match &argument as &str {
            "-h" => {
                println!("{}", HELP_MESSAGE);
                return;
            },

            "-ssk" => {
                show_skipped = true;
            }
            
            "-sum" => {
                show_unmatched = true;
            }

            "-mfs" => if next_argument_present {
                maximum_file_size = match next_argument.parse() {
                    Ok(value) => value,
                    Err(error) => {
                        panic!("Could not convert the provided maximum file size into an integer, error: {:?}", error);
                    }
                };
            }
            
            "-mfq" => if next_argument_present {
                maximum_files_queued = match next_argument.parse() {
                    Ok(value) => value,
                    Err(error) => {
                        panic!("Could not convert the provided maximum queued file count into an integer, error: {:?}", error);
                    }
                };
            }

            "-dir" => if next_argument_present {
                target_directory = next_argument.clone();
            }

            "-ext" => if next_argument_present {
                for extension in next_argument.split(":") {
                    file_extensions.push(String::from(extension));
                }
            }

            "-spt" => if next_argument_present {
                loop {
                    match argument_iterator.next() {
                        Some(pattern) => search_patterns.push(pattern),
                        None => break
                    };
                }
            }

            _ => {
                continue;
            }
        };
    }

    if search_patterns.len() > 0 {
        println!("Performing content search with the following parameters.");
        println!("\n{}", "-".repeat(50));
        println!("Search Patterns: {:?}", search_patterns);
        println!("Target Dir: {}", target_directory);
        println!("File Extensions: {:?}", file_extensions);
        println!("Max File Size: {}", maximum_file_size);
        println!("Max Queued Files: {}", maximum_files_queued);
        println!("{}", "-".repeat(50));

        let search_results:SearchResults = match perform_search(&target_directory, &file_extensions, &search_patterns, &maximum_file_size, &maximum_files_queued) {
            Ok(search_results) => search_results,
            Err(error) => {
                println!("perform_search Returned an error: {:?}", error);
                return;
            }
        };

        let matched_patterns_padsize:usize = match search_results.matched_files.iter().map(|matched_file| format!("{:?}", matched_file.matched_patterns)).max_by(|previous, current| previous.len().cmp(&current.len())) {
            Some(largest_string) => largest_string.len(),
            None => 0,
        };
    
        println!("{}", "-".repeat(50));
        
        if show_skipped {
            for skipped_file in &search_results.skipped_files {
                println!("SKIPPED({}) - {}", skipped_file.skip_reason, skipped_file.file_path);
            }
            
            println!("{}", "-".repeat(50));
        }
        
        if show_unmatched {
            for unmatched_file in &search_results.unmatched_files {
                println!("DIDN'T MATCH - {}", unmatched_file);
            }

            println!("{}", "-".repeat(50));
        }
    
        for matched_file in &search_results.matched_files {
            let mut matched_patterns_str:String = format!("{:?}", matched_file.matched_patterns);
    
            if matched_patterns_str.len() < matched_patterns_padsize {
                matched_patterns_str += " ".repeat(matched_patterns_padsize - matched_patterns_str.len()).as_str();
            }
    
            println!("{} | MATCHED IN > {}", matched_patterns_str, matched_file.file_path);
        }
    
        println!("{}", "-".repeat(50));

        println!("Matched {} files, {} unmatched candidates, {} files skipped.", search_results.matched_files.len(), search_results.unmatched_files.len(), search_results.skipped_files.len());
    } else {
        println!("Please specify at least one search pattern.");
    }
}