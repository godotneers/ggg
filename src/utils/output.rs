use terminal_size::terminal_size;

/// Prints a formatted table to stdout with automatic column width calculation.
///
/// This function takes column headers, a slice of data items, and extractor
/// functions that convert each data item into cell strings. It calculates optimal
/// column widths based on content length and available terminal width, distributing
/// space proportionally, according to each column's content needs. The table is then
/// printed with headers, a separator line, and aligned data rows.
///
/// # Parameters
///
/// * `columns` - Column headers for the table
/// * `data` - Slice of data items to display in the table
/// * `extractors` - Functions that extract string values from each data item for each column
///
/// # Panics
///
/// Panics if the number of extractors doesn't match the number of columns.
///
/// # Behavior
///
/// * Returns early if there are no rows or columns
/// * Calculates initial column widths by finding the maximum content length for each column
/// * Determines available terminal width (defaults to 80 columns if not detectable)
/// * Distributes available width proportionally based on each column's relative content needs
/// * Truncates cell content that exceeds the calculated column width
/// * Prints headers, followed by a separator line, then data rows
/// * Each cell is left-aligned with 2 spaces between columns
///
/// # Examples
///
/// ```
/// # use ggg::utils::output::print_table;
///
/// struct Package {
///     name: String,
///     version: String,
///     author: String,
/// }
///
/// let packages = vec![
///     Package {
///         name: "serde".to_string(),
///         version: "1.0.0".to_string(),
///         author: "dtolnay".to_string(),
///     },
///     Package {
///         name: "tokio".to_string(),
///         version: "1.28.0".to_string(),
///         author: "tokio-rs".to_string(),
///     },
/// ];
///
/// let columns = ["Name", "Version", "Author"];
/// let extractors: Vec<Box<dyn Fn(&Package) -> String>> = vec![
///     Box::new(|p| p.name.clone()),
///     Box::new(|p| p.version.clone()),
///     Box::new(|p| p.author.clone()),
/// ];
///
/// print_table(&columns, &packages, &extractors);
/// // Output:
/// // Name   Version  Author
/// // -----  -------  --------
/// // serde  1.0.0    dtolnay
/// // tokio  1.28.0   tokio-rs
/// ```
/// Extractors are boxed because heterogeneous closures each have a unique type,
/// so a single generic type cannot represent them.
#[allow(clippy::type_complexity)]
pub fn print_table<T>(
    columns: &[impl AsRef<str>],
    data: &[T],
    extractors: &[Box<dyn Fn(&T) -> String>],
) {
    let column_count = columns.len();
    let row_count = data.len();

    assert_eq!(
        column_count,
        extractors.len(),
        "Extractors and headers must be the same length"
    );

    if row_count == 0 || column_count == 0 {
        return;
    }

    let mut widths = vec![0; column_count];
    let mut extracted_data: Vec<Vec<String>> = vec![];

    // extract data and compute column widths
    for (i, item) in data.iter().enumerate() {
        if i == 0 {
            // add header row
            let mut header_row: Vec<String> = vec![];
            columns.iter().enumerate().for_each(|(j, col)| {
                let col_name = col.as_ref().to_string();
                widths[j] = col_name.len();
                header_row.push(col_name);
            });

            extracted_data.push(header_row);
        }

        // add a row
        let mut row: Vec<String> = vec![];
        for (j, extractor) in extractors.iter().enumerate() {
            let extracted_value = extractor(item);
            widths[j] = widths[j].max(extracted_value.len());
            row.push(extracted_value);
        }
        extracted_data.push(row);
    }

    // compute column width distributions based on available terminal width and
    // relative space needs
    let available_width = match terminal_size() {
        Some(it) => it.0.0 as usize,
        None => 80usize,
    } - 2 * (column_count - 1) // each column has a two-character separator
        - 1; // and we subtract one for the final newline

    let total_needed_width = widths.iter().sum::<usize>();

    // clamp widths
    for width in &mut widths {
        // calculate what fraction of the needed width the column takes up
        let fraction = *width as f64 / total_needed_width as f64;
        // and scale that to the available width, flooring to the nearest integer.
        *width = (fraction * available_width as f64).floor() as usize;
    }

    // print the table
    for (i, row) in extracted_data.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            let truncated: String = cell.chars().take(widths[j]).collect();
            print!("{:<width$}  ", truncated, width = widths[j]);
        }
        if i == 0 {
            println!();
            for (j, _) in row.iter().enumerate() {
                print!("{:-<width$}", "", width = widths[j],);
                if j < column_count - 1 {
                    print!("  ");
                }
            }
        }
        println!();
    }
}
