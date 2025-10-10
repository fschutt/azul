#!/bin/bash

# -----------------------------------------------------------------------------
# populate_files.sh
# -----------------------------------------------------------------------------
#
# This script parses a large Markdown file containing file paths and code blocks,
# and populates the actual files on the filesystem with the corresponding code.
#
# It operates on a whitelist to ensure only known "good quality" code is written.
#
# Usage:
#   ./populate_files.sh your_large_response.md
#
# Prerequisites:
#   - The Markdown file must be the first argument.
#   - The file structure (directories) should already exist. You can create it
#     with the `setup_props.sh` script from the previous response.
#
# -----------------------------------------------------------------------------

# --- Configuration ---

# The list of files that were identified as "good quality" and are safe to populate.
# Add or remove file paths here to control which files the script will write to.
WHITELIST=(
    "css/src/props/basic/angle.rs"
    "css/src/props/basic/direction.rs"
    "css/src/props/basic/color.rs"
    "css/src/props/basic/geometry.rs"
    "css/src/props/basic/animation.rs"
    "css/src/props/macros.rs"
)

# --- Script Logic ---

# Check for input file argument
if [[ -z "$1" ]]; then
    echo "Error: Please provide the path to the Markdown file as an argument."
    echo "Usage: $0 your_large_response.md"
    exit 1
fi

MARKDOWN_FILE="$1"

# Check if the markdown file exists
if [[ ! -f "$MARKDOWN_FILE" ]]; then
    echo "Error: Markdown file not found at '$MARKDOWN_FILE'"
    exit 1
fi

# Convert the bash array to a single, space-separated string for awk
# The weird syntax handles arrays correctly, even with spaces (though not needed here).
whitelist_str="${WHITELIST[*]}"

echo "Starting file population process..."
echo "Input file: $MARKDOWN_FILE"
echo "Processing ${#WHITELIST[@]} whitelisted files."
echo "---"

# Use awk to parse the markdown file and extract the code blocks
awk -v whitelist_str="$whitelist_str" '
BEGIN {
    # Create an awk associative array (hash map) from the whitelist string for fast lookups.
    split(whitelist_str, temp_arr, " ");
    for (i in temp_arr) {
        whitelist[temp_arr[i]] = 1;
    }
    
    # State variables
    is_writing = 0
    target_file = ""
}

# Match a heading that contains a file path in backticks
# Example: ### `css/src/props/basic/angle.rs`
/^### `/ {
    # Extract the filename from between the backticks
    if (match($0, /`([^`]+)`/)) {
        potential_file = substr($0, RSTART + 1, RLENGTH - 2);

        # Check if this file is in our whitelist
        if (potential_file in whitelist) {
            target_file = potential_file;
            print "[TARGET FOUND]   " target_file;
        } else {
            # If not in the whitelist, unset the target so we dont process its code block
            target_file = "";
            print "[SKIPPING]       " potential_file;
        }
    }
    # Skip to the next line to avoid processing the heading itself
    next;
}

# Match the start of a Rust code block
/^```rust/ {
    # Only start writing if we have a valid, whitelisted target file
    if (target_file != "") {
        # Set the writing flag and clear the target file for a fresh write
        is_writing = 1;
        # The first print action will overwrite the file
        close(target_file); # Ensure file is closed before writing to it again
        printf "" > target_file;
        print "                 -> Extracting code block...";
    }
    # Skip this line
    next;
}

# Match the end of any code block
/^```/ {
    # If we were writing, this is the end of the section.
    if (is_writing) {
        print "                 -> Done."
        # Reset all state variables
        is_writing = 0;
        target_file = "";
    }
    # Skip this line
    next;
}

# Default action for any other line
{
    # If the writing flag is set, append the current line to the target file.
    if (is_writing) {
        print $0 >> target_file;
    }
}
' "$MARKDOWN_FILE"

echo "---"
echo "Script finished. Check the files listed as [TARGET FOUND] above."