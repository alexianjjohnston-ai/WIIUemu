// Primitive-only wrapper around the AprilTag detector so the Rust side never depends on C struct layouts.
#include <stdlib.h>
#include <stdint.h>
#include "apriltag.h"
#include "tag36h11.h"
#include "common/image_u8.h"
#include "common/zarray.h"

typedef struct {
    apriltag_detector_t *td;
    apriltag_family_t *tf;
} wiiu_at_t;

wiiu_at_t *wiiu_at_create(int nthreads, float quad_decimate)
{
    wiiu_at_t *h = calloc(1, sizeof(wiiu_at_t));
    if (!h) return NULL;
    h->tf = tag36h11_create();
    h->td = apriltag_detector_create();
    if (!h->tf || !h->td) {
        if (h->td) apriltag_detector_destroy(h->td);
        if (h->tf) tag36h11_destroy(h->tf);
        free(h);
        return NULL;
    }
    apriltag_detector_add_family_bits(h->td, h->tf, 1);
    h->td->nthreads = nthreads > 0 ? nthreads : 1;
    h->td->quad_decimate = quad_decimate >= 1.0f ? quad_decimate : 1.0f;
    return h;
}

// Detects tags in an 8-bit grayscale image. Writes up to `max_out` results:
// ids[i], corners[i*8 .. i*8+8] as (x0,y0,x1,y1,x2,y2,x3,y3) in apriltag_detection_t.p order, margins[i].
// Returns the number of tags found (may exceed max_out), or -1 on failure.
int wiiu_at_detect(wiiu_at_t *h, const uint8_t *gray, int width, int height, int stride,
                   int max_out, int32_t *ids, double *corners, float *margins)
{
    if (!h || !gray || width <= 0 || height <= 0 || stride < width) return -1;
    image_u8_t im = { .width = width, .height = height, .stride = stride, .buf = (uint8_t *)gray };
    zarray_t *dets = apriltag_detector_detect(h->td, &im);
    if (!dets) return -1;
    int n = zarray_size(dets);
    for (int i = 0; i < n && i < max_out; i++) {
        apriltag_detection_t *det;
        zarray_get(dets, i, &det);
        ids[i] = det->id;
        margins[i] = det->decision_margin;
        for (int k = 0; k < 4; k++) {
            corners[i * 8 + 2 * k] = det->p[k][0];
            corners[i * 8 + 2 * k + 1] = det->p[k][1];
        }
    }
    apriltag_detections_destroy(dets);
    return n;
}

void wiiu_at_destroy(wiiu_at_t *h)
{
    if (!h) return;
    apriltag_detector_destroy(h->td);
    tag36h11_destroy(h->tf);
    free(h);
}
