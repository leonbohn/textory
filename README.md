# textory

**textory** is a command-line tool for generating a historical report of the page count of a LaTeX project across the commit history of a Git repository.


## Notes
Requires `latexmk` and git to be installed and available in your PATH.
The tool will prompt before overwriting existing output files.

## Usage Example

```sh
textory --main-tex-file thesis.tex --repo-path ~/my-beautiful-repo --output-dir ~/beautiful-repo-stats  --latexmk-args="-pdf"
```

#### Arguments
- `--main-tex-file, -m`
Path to the main LaTeX file (e.g., thesis.tex).
- `--repo-path, -r`
Path to the local Git repository.
- `--output-dir, -o`
Directory where output files (CSV and HTML report) will be written.
- `--latexmk-args`
Optional: Additional arguments to pass to latexmk (space-separated).

#### Output
- `textory_data.csv`: CSV file with columns Timestamp,Commit Hash,Page Count.
- `textory_report.html`: HTML report embedding the CSV data.
