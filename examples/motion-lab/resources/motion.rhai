// A small creator-owned behavior: seconds, radians, scene units.
fn properties() {
    #{ speed: 1.0, amplitude: 1.0 }
}

fn update(object, props, dt, time) {
    object.y = sin(time * props.speed) * props.amplitude;
    object.ry += props.speed * dt;
    object
}
