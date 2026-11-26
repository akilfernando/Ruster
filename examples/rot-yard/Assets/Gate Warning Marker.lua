local gate, template, marker

return {
    on_start = function(_, _, host)
        gate = host.find("Sliding Gate")
        template = host.find("Warning Marker Template")
    end,
    on_update = function(_, _, _, host)
        local button = host.button("slide_gate")
        if button.pressed and template and gate and not marker then
            marker = host.spawn(template, template.transform, gate)
        elseif not button.held and marker then
            host.destroy(marker)
            marker = nil
        end
    end,
}
