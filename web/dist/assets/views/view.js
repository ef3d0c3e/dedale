// What every view gets, and gives back.
/** The text of an error, for the banner. */
export function message(error) {
    return error instanceof Error ? error.message : String(error);
}
