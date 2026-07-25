use rand::{Rng, distributions::Alphanumeric};
//use std::fs::OpenOptions;
//use std::io::{self, Write};

use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};

fn main() -> io::Result<()> {
    // Open (or create) the file for appending
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("hashring.txt")?;

    // How many strings and how long each one should be
    let count = 10; // number of lines to write
    let length = 32; // characters per line

    // Thread‑local RNG – mutable because we’ll borrow it each iteration
    let mut rng = rand::thread_rng();

    for _ in 0..count {
        // Generate a random alphanumeric string
        let line: String = (&mut rng) // borrow mutably
            .sample_iter(&Alphanumeric) // iterator of random chars
            .take(length) // desired length
            .map(char::from) // convert u8 → char
            .collect();

        println!("{}", line);
        writeln!(file, "{}", line)?;
    }
    println!(
        "Generated {} additional Hashring nodes, each with a {} character-long identifier.",
        count, length
    );

    // -------------------------------------------------
    // 1. Read all lines from the source file
    // -------------------------------------------------
    let input_path = "hashring.txt";
    let file = OpenOptions::new().read(true).open(input_path)?;
    let reader = BufReader::new(file);

    let mut lines: Vec<String> = reader
        .lines() // iterator over Result<String>
        .collect::<Result<_, _>>()?; // turn into Vec<String> or propagate error

    lines.sort(); // default is ascending order

    let output_path = "hashring_sorted.txt";
    let mut out = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true) // overwrite if it already exists
        .open(output_path)?;

    for line in lines {
        writeln!(out, "{}", line)?;
    }

    let line_total = count_lines("hashring_sorted.txt")?;
    println!("The Hashring contains {line_total} nodes"); // → demo.txt contains 3 lines

    // locate the closest neighbors to a given hash on the hashring:
    let hiddenservice_hash = "ZZ3k9LmN0pQrStUvWxYz12ABcDeFgHiJ";
    println!(
        "The Hidden service gdatura24gtdy23lxd7ht3xzx6mi7mdlkabpvuefhrjn4t5jduviw5ad.dn's hash is {}",
        hiddenservice_hash
    );
    let (pos, nearest) = locate_nearest(hiddenservice_hash, "hashring_sorted.txt")?;

    println!(
        "The Hidden Service's rendezvous node DNS knowledge is most likely at the {}th position on the hashring.",
        pos
    );
    println!(
        "here are the 8 closest hashring neighbors of {} (that are the most likely to know where the RDV node of the hidden service is):",
        hiddenservice_hash
    );
    for neighbor in nearest {
        println!("  {neighbor}");
    }
    Ok(())
}

pub fn locate_nearest(target: &str, sorted_path: &str) -> io::Result<(usize, Vec<String>)> {
    // load all lines into memory
    let file = File::open(sorted_path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;

    let total = lines.len();
    if total == 0 {
        return Ok((0, Vec::new()));
    }

    // Binary search for insertion point
    let insert_idx = match lines.binary_search_by(|probe| probe.as_str().cmp(target)) {
        Ok(pos) => pos,  // exact match – insert before this position
        Err(pos) => pos, // not found – insert at `pos`
    };

    // Gather up to 8 neighbours with wrap‑around
    let mut neighbours = Vec::with_capacity(8);
    let mut taken = 0usize;
    let mut before = insert_idx as isize - 1; // start one element before
    let mut after = insert_idx; // start at insertion point

    while taken < 8 && (before >= 0 || after < total) {
        // Prefer the side that is still within bounds; if one side is exhausted,
        // continue taking from the other side (which may wrap).
        if before >= 0 {
            neighbours.push(lines[before as usize].clone());
            taken += 1;
            before -= 1;
            if taken == 8 {
                break;
            }
        }

        if after < total {
            neighbours.push(lines[after].clone());
            taken += 1;
            after += 1;
            if taken == 8 {
                break;
            }
        }

        // If both sides are exhausted (file smaller than 8 lines), break.
        if before < 0 && after >= total {
            break;
        }
    }

    // If we still have fewer than 8 neighbours because the file is tiny,
    // continue wrapping from the opposite end until we reach 8 or exhaust the file.
    while taken < 8 && total > neighbours.len() {
        // Wrap‑around from the start
        let wrap_idx = taken % total;
        neighbours.push(lines[wrap_idx].clone());
        taken += 1;
    }

    Ok((insert_idx, neighbours))
}

fn count_lines(path: &str) -> io::Result<usize> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    Ok(reader.lines().count())
}
