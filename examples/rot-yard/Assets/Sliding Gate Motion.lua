-- The panel and beacon share a parent, so both positions use the same local axes.
local updates, panel, beacon = 0, nil, nil
return {
    on_start = function(transform, props, host)
        panel = host.find("Sliding Gate Panel")
        beacon = host.find("Gate Beacon")
    end,
    on_update = function(transform, props, dt, host)
        updates = updates + 1
        if panel and beacon and host.button("slide_gate").held and updates > 1 then
            local pose = panel.transform
            local endpoint = beacon.transform.translation[1]
            local x = pose.translation[1]
            local distance = 2 * dt
            if x ~= endpoint then
                pose.translation[1] = x + math.max(-distance, math.min(distance, endpoint - x))
                panel:set_transform(pose)
            end
        end
    end,
}
