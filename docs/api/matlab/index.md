# MATLAB Client

!!! warning "Work in progress"
    The MATLAB client is under development. See `client/matlab/PLAN.md` for the design.

The MATLAB client will provide the same API surface as the Python client, using MATLAB's
protobuf support and a ZMQ binding.

## Planned usage

```matlab
conn = vstimd.Connection('tcp://localhost:5555');

h = conn.stimuli.create_rect('x', 0, 'y', 0, 'width', 200, 'height', 100, ...
                             'r', 1.0, 'g', 0.0, 'b', 0.0);
conn.stimuli.set_enabled(h, true);

info = conn.system.query_server_info();
fprintf('%dx%d @ %.1f Hz\n', info.width, info.height, info.frame_rate);

conn.stimuli.delete(h);
conn.close();
```

## Bonsai integration

vstimd can be driven from [Bonsai](https://bonsai-rx.org) via the ZMQ operator.
Documentation for the Bonsai integration will be added here.
