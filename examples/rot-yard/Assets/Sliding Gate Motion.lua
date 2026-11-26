-- Gate travel is authored in owner-local space.
local updates, active = 0, false
return {
    on_start = function(transform, props, host)
        active = true
    end,
    on_update = function(transform, props, dt, host)
        updates = updates + 1
        if host.button("slide_gate").held and updates > 1 then
            transform.translation[1] = math.min(3, transform.translation[1] + 2 * dt)
        end
    end,
}
