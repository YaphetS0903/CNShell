export function createTerminalInputQueue(send: (data: string) => Promise<void>) {
  let tail = Promise.resolve();
  return (data: string) => {
    const operation = tail.then(() => send(data));
    tail = operation.catch(() => undefined);
    return operation;
  };
}
