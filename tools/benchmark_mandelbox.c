// Host OpenCL timing/readback probe; no TRUEOS display or rig access.
#define main shadertoy_editor_main
#include "../../TRUEOS/tools/shadertoy-cpp-offline/main.c"
#undef main

static void require_cl(cl_int result, const char *operation) {
    if (result != CL_SUCCESS) {
        fprintf(stderr, "%s: OpenCL error %d\n", operation, result);
        exit(2);
    }
}

int main(int argc, char **argv) {
    if (argc != 3 && argc != 5) {
        fprintf(stderr, "usage: benchmark_mandelbox kernel.spv output-directory [width height]\n");
        return 2;
    }
    App *app = calloc(1, sizeof(*app));
    if (!app || !initialize_opencl(app)) {
        fprintf(stderr, "OpenCL init: %s\n", app ? app->status : "allocation failed");
        return 2;
    }
    ShaderRuntime *runtime = &app->runtime;
    char device_name[256] = {0};
    require_cl(clGetDeviceInfo(runtime->device, CL_DEVICE_NAME, sizeof(device_name), device_name, NULL), "device name");
    printf("device=%s\n", device_name);
    fflush(stdout);
    char *spirv = NULL;
    size_t spirv_size = 0;
    if (!read_file(argv[1], &spirv, &spirv_size)) return 2;
    cl_int error = 0;
    runtime->program = clCreateProgramWithIL(runtime->context, spirv, spirv_size, &error);
    free(spirv);
    require_cl(error, "IL program");
    error = clBuildProgram(runtime->program, 1, &runtime->device, NULL, NULL, NULL);
    if (error != CL_SUCCESS) {
        char log[16384] = {0};
        clGetProgramBuildInfo(runtime->program, runtime->device, CL_PROGRAM_BUILD_LOG, sizeof(log), log, NULL);
        fprintf(stderr, "%s\n", log);
        require_cl(error, "build");
    }
    runtime->kernel = clCreateKernel(runtime->program, "shadertoy_mandelbox", &error);
    require_cl(error, "kernel");
    const cl_uint width = argc == 5 ? (cl_uint)atoi(argv[3]) : 640;
    const cl_uint height = argc == 5 ? (cl_uint)atoi(argv[4]) : 360;
    if (width < 1 || height < 1 || width > 8192 || height > 8192) return 2;
    const cl_uint pitch = width * 4, atlas_width = 3078, atlas_height = 2052, atlas_pitch = atlas_width * 4;
    const size_t bytes = (size_t)pitch * height, atlas_bytes = (size_t)atlas_pitch * atlas_height;
    uint8_t *pixels = malloc(bytes);
    if (!pixels) return 2;
    struct { float values[16]; cl_uint controls[4]; float reserved[4]; } uniforms = {0};
    _Static_assert(sizeof(uniforms) == 96, "source-atlas uniform ABI");
    cl_mem atlas = clCreateBuffer(runtime->context, CL_MEM_READ_WRITE, atlas_bytes, NULL, &error);
    require_cl(error, "resident cubemap");
    runtime->output_buffer = clCreateBuffer(runtime->context, CL_MEM_READ_WRITE, bytes, NULL, &error);
    require_cl(error, "output buffer");
    runtime->uniform_buffer = clCreateBuffer(runtime->context, CL_MEM_READ_ONLY, sizeof(uniforms), NULL, &error);
    require_cl(error, "uniform buffer");
    require_cl(clSetKernelArg(runtime->kernel, 1, sizeof(runtime->uniform_buffer), &runtime->uniform_buffer), "uniform arg");
    require_cl(clSetKernelArg(runtime->kernel, 5, sizeof(atlas), &atlas), "source arg");
    const size_t global[2] = {(width + 15u) & ~15u, height}, local[2] = {16, 1};
    const char *presets[] = {"cathedral-single", "cathedral-duo", "cathedral-trio", "folded-void"};
    const char *views[] = {"front", "sky", "ground", "behind", "edge", "corner"};
    for (int preset = 0; preset < 4; ++preset) {
        memset(&uniforms, 0, sizeof(uniforms));
        uniforms.values[0] = atlas_width; uniforms.values[1] = atlas_height; uniforms.values[2] = 1;
        uniforms.values[8] = preset == 3 ? 0xd83cff : 0x63c7f2;
        uniforms.values[9] = 0x25153d; uniforms.values[10] = 0x4eaf68;
        uniforms.values[11] = preset != 3;
        uniforms.values[14] = preset == 3 ? 1 : preset + 1;
        uniforms.controls[0] = 1;
        uniforms.controls[1] = atlas_width; uniforms.controls[2] = atlas_height; uniforms.controls[3] = atlas_pitch;
        require_cl(clSetKernelArg(runtime->kernel, 0, sizeof(atlas), &atlas), "bake dst");
        require_cl(clSetKernelArg(runtime->kernel, 2, sizeof(atlas_width), &atlas_width), "bake width");
        require_cl(clSetKernelArg(runtime->kernel, 3, sizeof(atlas_height), &atlas_height), "bake height");
        require_cl(clSetKernelArg(runtime->kernel, 4, sizeof(atlas_pitch), &atlas_pitch), "bake pitch");
        require_cl(clEnqueueWriteBuffer(runtime->queue, runtime->uniform_buffer, CL_TRUE, 0, sizeof(uniforms), &uniforms, 0, NULL, NULL), "bake uniforms");
        uint64_t start = monotonic_nanoseconds();
        const size_t launched_width = (atlas_width + 15u) & ~15u;
        const size_t rows_per_batch = 16 * 1024 / launched_width;
        for (size_t row = 0; row < atlas_height; row += rows_per_batch) {
            size_t offset[2] = {0,row};
            size_t rows = atlas_height - row < rows_per_batch ? atlas_height - row : rows_per_batch;
            size_t bake_global[2] = {launched_width,rows};
            require_cl(clEnqueueNDRangeKernel(runtime->queue, runtime->kernel, 2, offset, bake_global, local, 0, NULL, NULL), "bake rows");
            require_cl(clFinish(runtime->queue), "bake retirement");
        }
        printf("preset=%s bake_ms=%.3f resident_bytes=%zu\n", presets[preset], (monotonic_nanoseconds()-start)/1000000.0, atlas_bytes);
        fflush(stdout);
        require_cl(clSetKernelArg(runtime->kernel, 0, sizeof(runtime->output_buffer), &runtime->output_buffer), "view dst");
        require_cl(clSetKernelArg(runtime->kernel, 2, sizeof(width), &width), "view width");
        require_cl(clSetKernelArg(runtime->kernel, 3, sizeof(height), &height), "view height");
        require_cl(clSetKernelArg(runtime->kernel, 4, sizeof(pitch), &pitch), "view pitch");
        uniforms.values[0] = width; uniforms.values[1] = height;
        uniforms.values[12] = 0.5773503f;
        uniforms.controls[0] = 2;
        for (int view = 0; view < 6; ++view) {
            const float half_pitch = view == 1 ? 0.7f : view == 2 ? -0.7f : view == 5 ? 0.30773985f : 0;
            const float half_yaw = view == 3 ? 1.57079633f : view >= 4 ? 0.39269908f : 0;
            uniforms.values[4] = sinf(half_pitch)*cosf(half_yaw);
            uniforms.values[5] = sinf(half_yaw)*cosf(half_pitch);
            uniforms.values[6] = -sinf(half_yaw)*sinf(half_pitch);
            uniforms.values[7] = cosf(half_yaw)*cosf(half_pitch);
            require_cl(clEnqueueWriteBuffer(runtime->queue, runtime->uniform_buffer, CL_TRUE, 0, sizeof(uniforms), &uniforms, 0, NULL, NULL), "view uniforms");
            double total_ms = 0;
            for (int frame = 0; frame < 6; ++frame) {
                start = monotonic_nanoseconds();
                require_cl(clEnqueueNDRangeKernel(runtime->queue, runtime->kernel, 2, NULL, global, local, 0, NULL, NULL), "view dispatch");
                require_cl(clFinish(runtime->queue), "view retirement");
                if (frame) total_ms += (monotonic_nanoseconds()-start)/1000000.0;
            }
            require_cl(clEnqueueReadBuffer(runtime->queue, runtime->output_buffer, CL_TRUE, 0, bytes, pixels, 0, NULL, NULL), "read frame");
            char path[2048];
            snprintf(path, sizeof(path), "%s/%s-%s.ppm", argv[2], presets[preset], views[view]);
            FILE *output = fopen(path, "wb");
            if (!output) return 2;
            fprintf(output, "P6\n%u %u\n255\n", width, height);
            for (size_t offset = 0; offset < bytes; offset += 4) {
                if (fwrite(pixels + offset, 1, 3, output) != 3) return 2;
            }
            if (fclose(output) != 0) return 2;
            // Preserve alpha for mask validation; PPM alone hides whether
            // the gaps are opaque black or truly transparent.
            snprintf(path, sizeof(path), "%s/%s-%s.rgba", argv[2], presets[preset], views[view]);
            output = fopen(path, "wb");
            if (!output || fwrite(pixels, 1, bytes, output) != bytes) return 2;
            if (fclose(output) != 0) return 2;
            printf("preset=%s view=%s warm_sample_ms=%.3f output=%s\n", presets[preset], views[view], total_ms/5.0, path);
            fflush(stdout);
        }
    }
    clReleaseMemObject(atlas); free(pixels); release_runtime(runtime); free(app);
    return 0;
}
