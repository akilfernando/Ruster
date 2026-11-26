-- The panel and beacon share a parent, so both positions use the same local axes.
local panel, beacon, light
return {
    on_start = function(transform, props, host)
        panel = host.find("Sliding Gate Panel")
        beacon = host.find("Gate Beacon")
        if beacon then
            light = beacon:component("Light", "gate-beacon-light")
        end
    end,
    on_update = function(transform, props, dt, host)
        local held = host.button("slide_gate").held
        local was_lit = not light or light.intensity == 6
        if light then
            light:set_intensity(held and 6 or 0.25)
        end
        if panel and beacon and held and was_lit then
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
