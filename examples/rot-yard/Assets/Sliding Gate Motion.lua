-- The panel and beacon share a parent, so both positions use the same local axes.
local updates, beacon = 0, nil
return {
    on_start = function(transform, props, host)
        beacon = host.find("Gate Beacon")
    end,
    on_update = function(transform, props, dt, host)
        updates = updates + 1
        if beacon and host.button("slide_gate").held and updates > 1 then
            local endpoint = beacon.transform.translation[1]
            local x = transform.translation[1]
            local distance = 2 * dt
            transform.translation[1] = x + math.max(-distance, math.min(distance, endpoint - x))
        end
    end,
}
