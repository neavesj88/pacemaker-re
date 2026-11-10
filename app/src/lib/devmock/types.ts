// A mock command handler: gets the invoke() arguments, returns the result (or throws a string,
// like a failing Rust command). Each area exports `handlers`, keyed by command name.
export type Handler = (args: Record<string, any>) => unknown;
export type Handlers = Record<string, Handler>;
