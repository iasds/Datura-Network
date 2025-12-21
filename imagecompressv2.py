#!/usr/bin/python3

# requires imagemagick, avifenc, svt-av1
# sudo apt install imagemagick svt-av1 libavif-bin

import os
import sys
import subprocess
import concurrent.futures

# below this threshold, it won't be compressed with avif
MIN_FILE_SIZE = 30000

def detect_compressible(fpath):
    mimetype = subprocess.check_output(["file", "--mime-type", "-b", fpath]).strip().decode('utf-8')

    if mimetype in ('image/png', 'image/jpeg'):
        return True

    if mimetype == 'image/webp':
        fileout = subprocess.check_output(["file", fpath]).decode('utf-8')
        if 'lossless' in fileout:
            return True

    return False

def compress(fpath):
    rtpath = '/tmp/'+os.urandom(4).hex()+'.png'
    subprocess.run(["convert", fpath, rtpath])
    subprocess.run(["avifenc", rtpath, "--yuv", "420", "--range", "l", "-c", "svt", "-j", "1", "--speed", "0", "--ignore-exif", "-o", fpath])
    try:
        os.remove(rtpath)
    except:
        pass

def process_files_in_directory(directory):
    compressible_files = []

    for root, _, files in os.walk(directory):
        for fi in files:
            full_path = os.path.join(root, fi)
            print(f"Checking file: {full_path}")
            if os.path.getsize(full_path) < MIN_FILE_SIZE:
                continue
            if detect_compressible(full_path):
                compressible_files.append(full_path)

    return compressible_files

def main(directory, max_workers):
    compressible_files = process_files_in_directory(directory)

    with concurrent.futures.ThreadPoolExecutor(max_workers=max_workers) as executor:
        futures = {executor.submit(compress, fpath): fpath for fpath in compressible_files}

        for future in concurrent.futures.as_completed(futures):
            fpath = futures[future]
            try:
                future.result()
                print(f"Successfully compressed: {fpath}")
            except Exception as exc:
                print(f"Compression failed for {fpath}: {exc}")

if __name__ == "__main__":
    # ./imagecompressv2.py [/path/to/dir] [num_workers]
    img_path = '.'
    workers = 4
    if len(sys.argv) > 1:
        img_path = sys.argv[1]
    if len(sys.argv) > 2:
        workers = int(sys.argv[2])
    main(img_path, max_workers=workers)
