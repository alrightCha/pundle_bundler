#!/bin/bash

# Input parameters
owner="$1"

# Create the directory for output if it doesn't exist
mkdir -p "accounts/$owner"

# Navigate to the directory
cd "accounts/$owner" || { echo "Failed to navigate to directory" >&2; exit 1; }

# Delete all files in the directory if they exist
if [[ -n $(ls -A) ]]; then
  echo "Deleting existing files in accounts/$owner..."
  rm -f ./*
fi

# Run the solana-keygen command with multiple threads
solana-keygen grind --ends-with p:1

# Check if the key was generated successfully
if [[ $? -eq 0 ]]; then
  # Find the latest .json file created, assuming it's the new keypair file
  keypair_file=$(ls -t | grep '.json' | head -n 1)
  if [[ -n $keypair_file ]]; then
    # Ensure the file is in the correct location
    keypair_dir=${keypair_file%.json}
    echo "new directory: $keypair_dir"
    mkdir "$keypair_dir"
    mkdir $keypair_file
    mv "$keypair_file" "./$keypair_dir"
    chmod 777 "accounts/$owner/$keypair_file"
    echo "Keypair generated at: accounts/$owner/$keypair_file"
    exit 0  # Exit with status 0 for success
  else
    echo "Keypair file not found" >&2
    exit 1  # Exit with status 1 because the file was not found
  fi
else
  echo "Failed to generate keypair" >&2
  exit 1  # Exit with status 1 for failure
fi
