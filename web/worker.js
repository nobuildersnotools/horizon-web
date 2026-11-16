// One of the game's threads (horizon-os's `workers`): instantiates the game's
// module on the page's shared memory, then runs the thread it was handed.
// The page names this build's glue by its hash (`?glue=`), so a cache that
// kept another build's can't pair it with this module.
self.onmessage = async ({ data }) => {
    self.onmessage = null;
    const glue = new URLSearchParams(self.location.search).get("glue");
    const { default: init, horizon_worker_entry } =
        await import(glue ? `./pkg/horizon-client.js?v=${glue}` : "./pkg/horizon-client.js");
    await init({ module_or_path: data.module, memory: data.memory, thread_stack_size: data.stack });
    horizon_worker_entry(data.ptr);
    self.close();
};
