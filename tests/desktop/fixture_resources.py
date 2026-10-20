"""Close fixture-owned parent pipe endpoints only after their child is reaped."""
def close_reaped_pipes(processes):
    for process in processes:
        if process.poll() is None:
            raise RuntimeError('Fixture child must be reaped before its pipes are closed')
        seen=set()
        for stream in (process.stdin,process.stdout,process.stderr):
            if stream is not None and id(stream) not in seen:
                seen.add(id(stream))
                stream.close()
