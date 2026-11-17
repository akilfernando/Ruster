-- Properties: speed (Number), enabled (Bool), target (ObjectRef).
return {
    update = function(transform, props, dt, host)
        if not props.enabled then return transform end
        local target = host.resolve(props.target)
        if not target then return transform end
        local distance = target.translation[1] - transform.translation[1]
        local step = math.max(0, props.speed) * dt
        transform.translation[1] = transform.translation[1]
            + math.min(step, math.max(-step, distance))
        return transform
    end
}
