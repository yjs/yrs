#include <cstdint>
#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN

#include <stdio.h>
#include <string.h>
#include "include/doctest.h"

extern "C" {
#include "include/libyrs.h"
};

YDoc *ydoc_new_with_id(uint64_t id) {
    YOptions o = yoptions();
    o.encoding = Y_OFFSET_UTF16;
    o.id = id;
    o.skip_gc = 0;

    return ydoc_new_with_options(o);
}

void exchange_updates(int len, ...) {
    va_list args;
    va_start(args, len);
    YDoc **docs = (YDoc **) malloc(sizeof(YDoc *) * len);
    for (int i = 0; i < len; i++) {
        docs[i] = va_arg(args, YDoc*);
    }
    va_end(args);

    char EXCHANGE_UPDATES[] = "exchange";
    for (int i = 0; i < len; i++) {
        for (int j = 0; j < len; j++) {
            if (i != j) {
                YDoc *d1 = docs[i];
                YDoc *d2 = docs[j];

                YTransaction *t1 = ydoc_read_transaction(d1);
                uint32_t sv1_len = 0;
                char *sv1 = ytransaction_state_vector_v1(t1, &sv1_len);
                ytransaction_commit(t1);

                YTransaction *t2 = ydoc_read_transaction(d2);
                uint32_t u2_len = 0;
                char *u2 = ytransaction_state_diff_v1(t2, sv1, sv1_len, &u2_len);
                ytransaction_commit(t2);
                ybinary_destroy(sv1, sv1_len);

                t1 = ydoc_write_transaction(d1, strlen(EXCHANGE_UPDATES), EXCHANGE_UPDATES);
                ytransaction_apply(t1, u2, u2_len);
                ytransaction_commit(t1);
                ybinary_destroy(u2, u2_len);
            }
        }
    }

    free(docs);
}

TEST_CASE("Update exchange basic") {
    // init
    YDoc *d1 = ydoc_new_with_id(1);
    Branch *txt1 = ytext(d1, "test");
    YTransaction *t1 = ydoc_write_transaction(d1, 0, NULL);

    YDoc *d2 = ydoc_new_with_id(2);
    Branch *txt2 = ytext(d2, "test");
    YTransaction *t2 = ydoc_write_transaction(d2, 0, NULL);

    // insert data at the same position on both peer texts
    ytext_insert(txt1, t1, 0, "world", NULL);
    ytext_insert(txt2, t2, 0, "hello ", NULL);

    // exchange updates
    uint32_t sv1_len = 0;
    char *sv1 = ytransaction_state_vector_v1(t1, &sv1_len);

    uint32_t sv2_len = 0;
    char *sv2 = ytransaction_state_vector_v1(t2, &sv2_len);

    uint32_t u1_len = 0;
    char *u1 = ytransaction_state_diff_v1(t1, sv2, sv2_len, &u1_len);

    uint32_t u2_len = 0;
    char *u2 = ytransaction_state_diff_v1(t2, sv1, sv1_len, &u2_len);

    ybinary_destroy(sv1, sv1_len);
    ybinary_destroy(sv2, sv2_len);

    // apply updates
    ytransaction_apply(t1, u2, u2_len);
    ytransaction_apply(t2, u1, u1_len);

    ybinary_destroy(u1, u1_len);
    ybinary_destroy(u2, u2_len);

    // make sure both peers produce the same output
    char *str1 = ytext_string(txt1, t1);
    char *str2 = ytext_string(txt2, t2);

    REQUIRE(!strcmp(str1, str2));

    ystring_destroy(str1);
    ystring_destroy(str2);

    ytransaction_commit(t2);
    ydoc_destroy(d2);

    ytransaction_commit(t1);
    ydoc_destroy(d1);
}

TEST_CASE("YText basic") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *txt = ytext(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    ytext_insert(txt, txn, 0, "hello", NULL);
    ytext_insert(txt, txn, 5, " world", NULL);
    ytext_remove_range(txt, txn, 0, 6);

    REQUIRE_EQ(ytext_len(txt, txn), 5);

    char *str = ytext_string(txt, txn);
    REQUIRE(!strcmp(str, "world"));

    ystring_destroy(str);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YArray basic") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *arr = yarray(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput *nested = (YInput *) malloc(2 * sizeof(YInput));
    nested[0] = yinput_float(0.5);
    nested[1] = yinput_bool(1);
    YInput nested_array = yinput_yarray(nested, 2);

    const int ARG_LEN = 3;

    YInput *args = (YInput *) malloc(ARG_LEN * sizeof(YInput));
    args[0] = nested_array;
    args[1] = yinput_string("hello");
    args[2] = yinput_long(123);

    yarray_insert_range(arr, txn, 0, args, ARG_LEN); //state after: [ YArray([0.5, true]), 'hello', 123]

    free(nested);
    free(args);

    yarray_remove_range(arr, txn, 1, 1); //state after: [ YArray([0.5, true]), 123 ]

    REQUIRE_EQ(yarray_len(arr), 2);

    YArrayIter *i = yarray_iter(arr, txn);

    // first outer YArray element should be another YArray([0.5, true])
    YOutput *curr = yarray_iter_next(i);
    Branch *a = youtput_read_yarray(curr);
    REQUIRE_EQ(yarray_len(a), 2);

    // read 0th element of inner YArray
    YOutput *elem = yarray_get(a, txn, 0);
    REQUIRE_EQ(*youtput_read_float(elem), 0.5);
    youtput_destroy(elem);

    // read 1st element of inner YArray
    elem = yarray_get(a, txn, 1);
    REQUIRE_EQ(*youtput_read_bool(elem), 1); // in C we use 1 to mark TRUE
    youtput_destroy(elem);
    youtput_destroy(curr);

    // second outer YArray element should be 123
    curr = yarray_iter_next(i);
    REQUIRE_EQ(*youtput_read_long(curr), 123);
    youtput_destroy(curr);

    curr = yarray_iter_next(i);
    REQUIRE(curr == NULL);

    yarray_iter_destroy(i);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YMap basic") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    // insert 'a' => 'value'
    YInput a = yinput_string("value");

    ymap_insert(map, txn, "a", &a);

    // insert 'b' -> [11,22]
    YInput *array = (YInput *) malloc(2 * sizeof(YInput));
    array[0] = yinput_long(11);
    array[1] = yinput_long(22);
    YInput b = yinput_json_array(array, 2);

    ymap_insert(map, txn, "b", &b);
    free(array);

    REQUIRE_EQ(ymap_len(map, txn), 2);

    // iterate over entries
    YMapIter *i = ymap_iter(map, txn);
    YMapEntry *curr;

    YMapEntry **acc = (YMapEntry **) malloc(2 * sizeof(YMapEntry *));
    acc[0] = ymap_iter_next(i);
    acc[1] = ymap_iter_next(i);

    curr = ymap_iter_next(i);
    REQUIRE(curr == NULL);

    ymap_iter_destroy(i);

    for (int i = 0; i < 2; i++) {
        curr = acc[i];
        switch (curr->key[0]) {
            case 'a': {
                REQUIRE(!strcmp(curr->key, "a"));
                REQUIRE(!strcmp(youtput_read_string(curr->value), "value"));
                break;
            }
            case 'b': {
                REQUIRE(!strcmp(curr->key, "b"));
                REQUIRE_EQ(curr->value->len, 2);
                YOutput *output = youtput_read_json_array(curr->value);
                YOutput *fst = &output[0];
                YOutput *snd = &output[1];
                REQUIRE_EQ(*youtput_read_long(fst), 11);
                REQUIRE_EQ(*youtput_read_long(snd), 22);
                break;
            }
            default: {
                FAIL("Unrecognized case: ", curr->key);
                break;
            }
        }
        ymap_entry_destroy(curr);
    }

    free(acc);

    // remove 'a' twice - second attempt should return null
    char removed = ymap_remove(map, txn, "a");
    REQUIRE_EQ(removed, 1);

    removed = ymap_remove(map, txn, "a");
    REQUIRE_EQ(removed, 0);

    // get 'b' and read its contents
    YOutput *out = ymap_get(map, txn, "b");
    YOutput *output = youtput_read_json_array(out);
    REQUIRE_EQ(out->len, 2);
    REQUIRE_EQ(*youtput_read_long(&output[0]), 11);
    REQUIRE_EQ(*youtput_read_long(&output[1]), 22);
    youtput_destroy(out);

    // clear map
    ymap_remove_all(map, txn);
    REQUIRE_EQ(ymap_len(map, txn), 0);

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YXmlElement basic") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *frag = yxmlfragment(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);
    Branch *xml = yxmlelem_insert_elem(frag, txn, 0, "div");

    // XML attributes API
    yxmlelem_insert_attr(xml, txn, "key1", "value1");
    yxmlelem_insert_attr(xml, txn, "key2", "value2");

    YXmlAttrIter *i = yxmlelem_attr_iter(xml, txn);
    YXmlAttr *attr;

    YXmlAttr **attrs = (YXmlAttr **) malloc(2 * sizeof(YXmlAttr *));
    attrs[0] = yxmlattr_iter_next(i);
    attrs[1] = yxmlattr_iter_next(i);

    attr = yxmlattr_iter_next(i);
    REQUIRE(attr == NULL);
    yxmlattr_destroy(attr);

    for (int j = 0; j < 2; ++j) {
        attr = attrs[j];
        switch (attr->name[3]) {
            case '1': {
                REQUIRE(!strcmp(attr->name, "key1"));
                REQUIRE(!strcmp(attr->value, "value1"));
                break;
            }
            case '2': {
                REQUIRE(!strcmp(attr->name, "key2"));
                REQUIRE(!strcmp(attr->value, "value2"));
                break;
            }
            default: {
                FAIL("Unrecognized attribute name: ", attr->name);
                break;
            }
        }
        yxmlattr_destroy(attr);
    }

    // XML children API
    Branch *inner = yxmlelem_insert_elem(xml, txn, 0, "p");
    Branch *inner_txt = yxmlelem_insert_text(inner, txn, 0);
    yxmltext_insert(inner_txt, txn, 0, "hello", NULL);

    REQUIRE_EQ(yxmlelem_child_len(xml, txn), 1);

    Branch *txt = yxmlelem_insert_text(xml, txn, 1);
    yxmltext_insert(txt, txn, 0, "world", NULL);

    // check tag names
    char *tag = yxmlelem_tag(inner);
    REQUIRE(!strcmp(tag, "p"));
    ystring_destroy(tag);

    tag = yxmlelem_tag(xml);
    REQUIRE(!strcmp(tag, "div"));
    ystring_destroy(tag);

    // check parents
    Branch *parent = yxmlelem_parent(inner);
    tag = yxmlelem_tag(parent);
    REQUIRE(!strcmp(tag, "div"));
    ystring_destroy(tag);

    parent = yxmlelem_parent(xml);
    REQUIRE(parent != NULL);

    // check children traversal
    YOutput *curr = yxmlelem_first_child(xml);
    Branch *first = youtput_read_yxmlelem(curr);
    REQUIRE(yxml_prev_sibling(first, txn) == NULL);
    char *str = yxmlelem_string(first, txn);
    REQUIRE(!strcmp(str, "<p>hello</p>"));
    ystring_destroy(str);

    YOutput *next = yxml_next_sibling(first, txn);
    youtput_destroy(curr);
    Branch *second = youtput_read_yxmltext(next);
    REQUIRE(yxml_next_sibling(second, txn) == NULL);
    str = yxmltext_string(second, txn);
    REQUIRE(!(strcmp(str, "world")));
    ystring_destroy(str);

    // check tree walker - expected order:
    // - p
    // - hello
    // - world
    YXmlTreeWalker *w = yxmlelem_tree_walker(xml, txn);
    Branch *e;

    curr = yxmlelem_tree_walker_next(w);
    e = youtput_read_yxmlelem(curr);
    str = yxmlelem_string(e, txn);
    REQUIRE(!strcmp(str, "<p>hello</p>"));
    ystring_destroy(str);
    youtput_destroy(curr);

    curr = yxmlelem_tree_walker_next(w);
    Branch *t = youtput_read_yxmltext(curr);
    str = yxmltext_string(t, txn);
    REQUIRE(!strcmp(str, "hello"));
    ystring_destroy(str);
    youtput_destroy(curr);

    curr = yxmlelem_tree_walker_next(w);
    t = youtput_read_yxmltext(curr);
    str = yxmltext_string(t, txn);
    REQUIRE(!strcmp(str, "world"));
    ystring_destroy(str);
    youtput_destroy(curr);

    curr = yxmlelem_tree_walker_next(w);
    REQUIRE(curr == NULL);

    yxmlelem_tree_walker_destroy(w);

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

typedef struct YTextEventTest {
    uint32_t delta_len;
    YDeltaOut *delta;
    Branch *target;
} YEventTest;

YTextEventTest *ytext_event_test_new() {
    YTextEventTest *t = (YTextEventTest *) malloc(sizeof(YTextEventTest));
    t->target = NULL;
    t->delta = NULL;
    t->delta_len = 0;

    return t;
}

void ytext_test_observe(void *state, const YTextEvent *e, uint32_t origin_len, const char *origin) {
    YTextEventTest *t = (YTextEventTest *) state;
    t->target = ytext_event_target(e);
    t->delta = ytext_event_delta(e, &t->delta_len);
}

void ytext_test_clean(YTextEventTest *t) {
    ytext_delta_destroy(t->delta, t->delta_len);
    t->target = NULL;
    t->delta = NULL;
}

TEST_CASE("YText observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *txt = ytext(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YTextEventTest *t = ytext_event_test_new();
    YSubscription *sub = ytext_observe(txt, (void *) t, &ytext_test_observe);

    // insert initial data to an empty YText
    ytext_insert(txt, txn, 0, "abcd", NULL);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 1);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[0].insert->len == 4);

    // remove 2 chars from the middle
    ytext_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    ytext_remove_range(txt, txn, 1, 2);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_DELETE);
    REQUIRE(t->delta[1].len == 2);

    // insert new item in the middle
    ytext_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    ytext_insert(txt, txn, 1, "e", NULL);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[1].len == 1);

    // free the observer and make sure that callback is no longer called
    ytext_test_clean(t);
    yunobserve(sub);

    txn = ydoc_write_transaction(doc, 0, NULL);
    ytext_insert(txt, txn, 1, "fgh", NULL);
    ytransaction_commit(txn);

    REQUIRE(t->target == NULL);
    REQUIRE(t->delta == NULL);

    free(t);
    ydoc_destroy(doc);
}


TEST_CASE("YText insert delta") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *f = yxmlfragment(doc, "frag");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);
    Branch *txt = yxmlelem_insert_text(f, txn, 0);

    // initial text state
    ytext_insert(txt, txn, 0, "halo world", NULL);

    // construct delta
    char *bold_str = "bold";
    YInput bold = yinput_bool(Y_TRUE);
    const YInput attrs = yinput_json_map(&bold_str, &bold, 1);

    YDeltaIn i0 = ydelta_input_retain(1, NULL);
    YDeltaIn i1 = ydelta_input_delete(1);
    const YInput typo_fix = yinput_string("el");
    YDeltaIn i2 = ydelta_input_insert(&typo_fix, NULL);
    YDeltaIn i3 = ydelta_input_retain(3, NULL);
    YDeltaIn i4 = ydelta_input_retain(5, &attrs);
    YDeltaIn delta[5] = {i0, i1, i2, i3, i4};

    ytext_insert_delta(txt, txn, delta, 5);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);
    char *str = yxmltext_string(txt, txn);

    REQUIRE(strcmp(str, "hello <bold>world</bold>") == 0);

    ystring_destroy(str);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YText insert embed") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *txt = ytext(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YTextEventTest *t = ytext_event_test_new();
    YSubscription *sub = ytext_observe(txt, (void *) t, &ytext_test_observe);

    char *_bold = (char *) "bold";
    YInput _true = yinput_bool(1);
    YInput attrs1 = yinput_json_map(&_bold, &_true, 1);

    char *_width = (char *) "width";
    YInput _100 = yinput_long(100);
    YInput attrs2 = yinput_json_map(&_width, &_100, 1);

    char *_image = (char *) "image";
    YInput _image_src = yinput_string("imageSrc.png");
    YInput embed = yinput_json_map(&_image, &_image_src, 1);

    ytext_insert(txt, txn, 0, "ab", &attrs1);
    ytext_insert_embed(txt, txn, 1, &embed, &attrs2);
    ytransaction_commit(txn);

    REQUIRE(t->delta_len == 3);

    YDeltaOut d = t->delta[0];
    REQUIRE(d.tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(d.insert->len == 1);
    REQUIRE(strcmp(youtput_read_string(d.insert), "a") == 0);
    REQUIRE(d.attributes_len == 1);
    REQUIRE(strcmp(d.attributes->key, "bold") == 0);
    REQUIRE(*youtput_read_bool(&d.attributes->value) == 1);

    d = t->delta[1];
    REQUIRE(d.tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(d.len == 1);
    REQUIRE(d.attributes_len == 1);
    REQUIRE(strcmp(d.attributes->key, "width") == 0);
    REQUIRE(*youtput_read_long(&d.attributes->value) == 100);
    YMapEntry *e = youtput_read_json_map(d.insert);
    REQUIRE(d.insert->len == 1);
    REQUIRE(strcmp(e->key, "image") == 0);
    REQUIRE(strcmp(youtput_read_string(e->value), "imageSrc.png") == 0);

    d = t->delta[2];
    REQUIRE(d.tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(d.insert->len == 1);
    REQUIRE(strcmp(youtput_read_string(d.insert), "b") == 0);
    REQUIRE(d.attributes_len == 1);
    REQUIRE(strcmp(d.attributes->key, "bold") == 0);
    REQUIRE(*youtput_read_bool(&d.attributes->value) == 1);

    ytext_test_clean(t);
    yunobserve(sub);
    free(t);
    ydoc_destroy(doc);
}

TEST_CASE("YText formatting") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *txt = ytext(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    char i[] = "i";
    char *keysi[] = {i};
    char b[] = "b";
    char *keysb[] = {b};
    YInput yes = yinput_bool(Y_TRUE);
    YInput italic = yinput_json_map(keysi, &yes, 1);
    YInput bold = yinput_json_map(keysb, &yes, 1);

    ytext_insert(txt, txn, 0, "hello world!", &italic);
    ytext_format(txt, txn, 6, 5, &bold);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);
    uint32_t chunks_len;
    YChunk *chunks = ytext_chunks(txt, txn, &chunks_len);
    ytransaction_commit(txn);

    REQUIRE_EQ(chunks_len, 3);
    YChunk chunk = chunks[0];
    REQUIRE(strcmp(youtput_read_string(&chunk.data), "hello ") == 0);
    REQUIRE_EQ(chunk.fmt_len, 1);
    REQUIRE(strcmp(chunk.fmt[0].key, i) == 0);
    REQUIRE_EQ(*youtput_read_bool(chunk.fmt[0].value), Y_TRUE);

    chunk = chunks[1];
    REQUIRE(strcmp(youtput_read_string(&chunk.data), "world") == 0);
    REQUIRE_EQ(chunk.fmt_len, 2);
    for (int i = 0; i < chunk.fmt_len; i++) {
        YMapEntry e = chunk.fmt[i];
        REQUIRE_EQ(*youtput_read_bool(e.value), Y_TRUE);
        REQUIRE_EQ(strlen(e.key), 1);
        REQUIRE((e.key[0] == 'i' || e.key[0] == 'b'));
    }
    REQUIRE_EQ(*youtput_read_bool(chunk.fmt[0].value), Y_TRUE);

    chunk = chunks[2];
    REQUIRE(strcmp(youtput_read_string(&chunk.data), "!") == 0);
    REQUIRE_EQ(chunk.fmt_len, 1);
    REQUIRE(strcmp(chunk.fmt[0].key, i) == 0);
    REQUIRE_EQ(*youtput_read_bool(chunk.fmt[0].value), Y_TRUE);

    ychunks_destroy(chunks, chunks_len);
    ydoc_destroy(doc);
}

typedef struct YArrayEventTest {
    uint32_t delta_len;
    YEventChange *delta;
    Branch *target;
} YArrayEventTest;

YArrayEventTest *yarray_event_test_new() {
    YArrayEventTest *t = (YArrayEventTest *) malloc(sizeof(YArrayEventTest));
    t->target = NULL;
    t->delta = NULL;
    t->delta_len = 0;

    return t;
}

void yarray_test_observe(void *state, const YArrayEvent *e, uint32_t origin_len, const char *origin) {
    YArrayEventTest *t = (YArrayEventTest *) state;
    t->target = yarray_event_target(e);
    t->delta = yarray_event_delta(e, &t->delta_len);
}

void yarray_test_clean(YArrayEventTest *t) {
    yevent_delta_destroy(t->delta, t->delta_len);
    t->target = NULL;
    t->delta = NULL;
}

TEST_CASE("YArray observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *array = yarray(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YArrayEventTest *t = yarray_event_test_new();
    YSubscription *sub = yarray_observe(array, (void *) t, &yarray_test_observe);

    // insert initial data to an empty YArray
    YInput *i = (YInput *) malloc(4 * sizeof(YInput));
    i[0] = yinput_long(1);
    i[1] = yinput_long(2);
    i[2] = yinput_long(3);
    i[3] = yinput_long(4);

    yarray_insert_range(array, txn, 0, i, 4);
    ytransaction_commit(txn);
    free(i);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 1);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[0].len == 4);

    // remove 2 items from the middle
    yarray_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    yarray_remove_range(array, txn, 1, 2);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_DELETE);
    REQUIRE(t->delta[1].len == 2);

    yarray_test_clean(t);

    // insert new item in the middle
    i = (YInput *) malloc(1 * sizeof(YInput));
    i[0] = yinput_long(5);

    txn = ydoc_write_transaction(doc, 0, NULL);
    yarray_insert_range(array, txn, 1, i, 1);
    ytransaction_commit(txn);
    free(i);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[1].len == 1);

    yarray_test_clean(t);

    // free the observer and make sure that callback is no longer called
    yunobserve(sub);

    i = (YInput *) malloc(1 * sizeof(YInput));
    i[0] = yinput_long(5);

    txn = ydoc_write_transaction(doc, 0, NULL);
    yarray_insert_range(array, txn, 1, i, 1);
    ytransaction_commit(txn);
    free(i);

    REQUIRE(t->target == NULL);
    REQUIRE(t->delta == NULL);

    free(t);
    ydoc_destroy(doc);
}

typedef struct YMapEventTest {
    uint32_t keys_len;
    YEventKeyChange *keys;
    Branch *target;
} YMapEventTest;

YMapEventTest *ymap_event_test_new() {
    YMapEventTest *t = (YMapEventTest *) malloc(sizeof(YMapEventTest));
    t->target = NULL;
    t->keys = NULL;
    t->keys_len = 0;

    return t;
}

void ymap_test_observe(void *state, const YMapEvent *e, uint32_t origin_len, const char *origin) {
    YMapEventTest *t = (YMapEventTest *) state;
    t->target = ymap_event_target(e);
    t->keys = ymap_event_keys(e, &t->keys_len);
}

void ymap_test_clean(YMapEventTest *t) {
    yevent_keys_destroy(t->keys, t->keys_len);
    t->target = NULL;
    t->keys = NULL;
}

TEST_CASE("YMap observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YMapEventTest *t = ymap_event_test_new();
    YSubscription *sub = ymap_observe(map, (void *) t, &ymap_test_observe);

    // insert initial data to an empty YMap
    YInput i1 = yinput_string("value1");
    YInput i2 = yinput_long(2);
    ymap_insert(map, txn, "key1", &i1);
    ymap_insert(map, txn, "key2", &i2);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->keys_len == 2);

    for (int i = 0; i < t->keys_len; i++) {
        YEventKeyChange e = t->keys[i];
        switch (e.key[3]) {
            case '1': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_ADD);
                REQUIRE(!strcmp(e.key, "key1"));
                REQUIRE(e.old_value == NULL);
                REQUIRE(!strcmp(youtput_read_string(e.new_value), "value1"));
                break;
            }
            case '2': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_ADD);
                REQUIRE(!strcmp(e.key, "key2"));
                REQUIRE(e.old_value == NULL);
                REQUIRE(*youtput_read_long(e.new_value) == 2);
                break;
            }
            default:
                FAIL("unrecognized case");
        }
    }

    // remove an entry and update another on
    ymap_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    ymap_remove(map, txn, "key1");
    i2 = yinput_string("value2");
    ymap_insert(map, txn, "key2", &i2);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->keys_len == 2);

    for (int i = 0; i < t->keys_len; i++) {
        YEventKeyChange e = t->keys[i];
        switch (e.key[3]) {
            case '1': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_DELETE);
                REQUIRE(!strcmp(e.key, "key1"));
                REQUIRE(!strcmp(youtput_read_string(e.old_value), "value1"));
                REQUIRE(e.new_value == NULL);
                break;
            }
            case '2': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_UPDATE);
                REQUIRE(!strcmp(e.key, "key2"));
                REQUIRE((*youtput_read_long(e.old_value)) == 2);
                REQUIRE(!strcmp(youtput_read_string(e.new_value), "value2"));
                break;
            }
            default:
                FAIL("unrecognized case");
        }
    }

    // free the observer and make sure that callback is no longer called
    ymap_test_clean(t);
    yunobserve(sub);
    txn = ydoc_write_transaction(doc, 0, NULL);
    ymap_remove(map, txn, "key2");
    ytransaction_commit(txn);

    REQUIRE(t->target == NULL);
    REQUIRE(t->keys == NULL);

    ydoc_destroy(doc);
}

typedef struct YXmlTextEventTest {
    uint32_t delta_len;
    uint32_t keys_len;
    YDeltaOut *delta;
    Branch *target;
    YEventKeyChange *keys;
} YXmlTextEventTest;

YXmlTextEventTest *yxmltext_event_test_new() {
    YXmlTextEventTest *t = (YXmlTextEventTest *) malloc(sizeof(YXmlTextEventTest));
    t->target = NULL;
    t->delta = NULL;
    t->delta_len = 0;
    t->keys = NULL;
    t->keys_len = 0;

    return t;
}

void yxmltext_test_observe(void *state, const YXmlTextEvent *e, uint32_t origin_len, const char *origin) {
    YXmlTextEventTest *t = (YXmlTextEventTest *) state;
    t->target = yxmltext_event_target(e);
    t->delta = yxmltext_event_delta(e, &t->delta_len);
    t->keys = yxmltext_event_keys(e, &t->keys_len);
}

void yxmltext_test_clean(YXmlTextEventTest *t) {
    ytext_delta_destroy(t->delta, t->delta_len);
    yevent_keys_destroy(t->keys, t->keys_len);
    t->target = NULL;
    t->delta = NULL;
    t->keys = NULL;
}

TEST_CASE("YXmlText observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *frag = yxmlfragment(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);
    Branch *txt = yxmlelem_insert_text(frag, txn, 0);
    ytransaction_commit(txn);
    txn = ydoc_write_transaction(doc, 0, NULL);

    YXmlTextEventTest *t = yxmltext_event_test_new();
    YSubscription *sub = yxmltext_observe(txt, (void *) t, &yxmltext_test_observe);

    // insert initial data to an empty YText
    yxmltext_insert(txt, txn, 0, "abcd", NULL);
    ytransaction_commit(txn);

    REQUIRE(t->delta_len == 1);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[0].insert->len == 4);
    REQUIRE(t->target != NULL);

    // remove 2 chars from the middle
    yxmltext_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    yxmltext_remove_range(txt, txn, 1, 2);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_DELETE);
    REQUIRE(t->delta[1].len == 2);

    // insert new item in the middle
    yxmltext_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    yxmltext_insert(txt, txn, 1, "e", NULL);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[1].len == 1);

    // free the observer and make sure that callback is no longer called
    yxmltext_test_clean(t);
    yunobserve(sub);

    txn = ydoc_write_transaction(doc, 0, NULL);
    yxmltext_insert(txt, txn, 1, "fgh", NULL);
    ytransaction_commit(txn);

    REQUIRE(t->target == NULL);
    REQUIRE(t->delta == NULL);

    free(t);
    ydoc_destroy(doc);
}

typedef struct YXmlEventTest {
    Branch *target;
    uint32_t keys_len;
    uint32_t delta_len;
    YEventKeyChange *keys;
    YEventChange *delta;
} YXmlEventTest;

YXmlEventTest *yxml_event_test_new() {
    YXmlEventTest *t = (YXmlEventTest *) malloc(sizeof(YXmlEventTest));
    t->target = NULL;
    t->keys = NULL;
    t->keys_len = 0;
    t->delta_len = 0;
    t->delta = NULL;

    return t;
}

void yxml_test_observe(void *state, const YXmlEvent *e, uint32_t origin_len, const char *origin) {
    YXmlEventTest *t = (YXmlEventTest *) state;
    t->target = yxmlelem_event_target(e);
    t->keys = yxmlelem_event_keys(e, &t->keys_len);
    t->delta = yxmlelem_event_delta(e, &t->delta_len);
}

void yxml_test_clean(YXmlEventTest *t) {
    yevent_keys_destroy(t->keys, t->keys_len);
    yevent_delta_destroy(t->delta, t->delta_len);
    t->target = NULL;
    t->keys = NULL;
    t->delta = NULL;
    t->delta_len = 0;
    t->keys_len = 0;
}

TEST_CASE("YXmlElement observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *frag = yxmlfragment(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);
    Branch *xml = yxmlelem_insert_elem(frag, txn, 0, "div");
    ytransaction_commit(txn);
    txn = ydoc_write_transaction(doc, 0, NULL);

    YXmlEventTest *t = yxml_event_test_new();
    YSubscription *sub = yxmlelem_observe(xml, (void *) t, &yxml_test_observe);

    // insert initial attributes
    yxmlelem_insert_attr(xml, txn, "attr1", "value1");
    yxmlelem_insert_attr(xml, txn, "attr2", "value2");
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->keys_len == 2);

    for (int i = 0; i < t->keys_len; i++) {
        YEventKeyChange e = t->keys[i];
        switch (e.key[4]) {
            case '1': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_ADD);
                REQUIRE(!strcmp(e.key, "attr1"));
                REQUIRE(e.old_value == NULL);
                REQUIRE(!strcmp(youtput_read_string(e.new_value), "value1"));
                break;
            }
            case '2': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_ADD);
                REQUIRE(!strcmp(e.key, "attr2"));
                REQUIRE(e.old_value == NULL);
                REQUIRE(!strcmp(youtput_read_string(e.new_value), "value2"));
                break;
            }
            default:
                FAIL("unrecognized case");
        }
    }

    // update attributes
    yxml_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    yxmlelem_insert_attr(xml, txn, "attr1", "value11");
    yxmlelem_remove_attr(xml, txn, "attr2");
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->keys_len == 2);

    for (int i = 0; i < t->keys_len; i++) {
        YEventKeyChange e = t->keys[i];
        switch (e.key[4]) {
            case '1': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_UPDATE);
                REQUIRE(!strcmp(e.key, "attr1"));
                REQUIRE(!strcmp(youtput_read_string(e.old_value), "value1"));
                REQUIRE(!strcmp(youtput_read_string(e.new_value), "value11"));
                break;
            }
            case '2': {
                REQUIRE(e.tag == Y_EVENT_KEY_CHANGE_DELETE);
                REQUIRE(!strcmp(e.key, "attr2"));
                REQUIRE(!strcmp(youtput_read_string(e.old_value), "value2"));
                REQUIRE(e.new_value == NULL);
                break;
            }
            default:
                FAIL("unrecognized case");
        }
    }

    // add children
    yxml_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    Branch *div = yxmlelem_insert_elem(xml, txn, 0, "div");
    Branch *p = yxmlelem_insert_elem(xml, txn, 1, "p");
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 1);

    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[0].len == 2);

    // remove a child
    yxml_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    yxmlelem_remove_range(xml, txn, 1, 1);
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_DELETE);
    REQUIRE(t->delta[1].len == 1);

    // insert child again
    yxml_test_clean(t);
    txn = ydoc_write_transaction(doc, 0, NULL);
    div = yxmlelem_insert_elem(xml, txn, 1, "div");
    ytransaction_commit(txn);

    REQUIRE(t->target != NULL);
    REQUIRE(t->delta_len == 2);
    REQUIRE(t->delta[0].tag == Y_EVENT_CHANGE_RETAIN);
    REQUIRE(t->delta[0].len == 1);
    REQUIRE(t->delta[1].tag == Y_EVENT_CHANGE_ADD);
    REQUIRE(t->delta[1].len == 1);


    // free the observer and make sure that callback is no longer called
    yxml_test_clean(t);
    yunobserve(sub);
    txn = ydoc_write_transaction(doc, 0, NULL);
    Branch *inner = yxmlelem_insert_elem(xml, txn, 0, "head");
    ytransaction_commit(txn);

    REQUIRE(t->target == NULL);
    REQUIRE(t->delta == NULL);
    REQUIRE(t->keys == NULL);

    free(t);
    ydoc_destroy(doc);
}

typedef struct YDeepObserveTest {
    YPathSegment *path[4];
    uint32_t path_lens[4];
    uint32_t count;
} YDeepObserveTest;

YDeepObserveTest *new_ydeepobserve_test() {
    YDeepObserveTest *e = (YDeepObserveTest *) malloc(sizeof(YDeepObserveTest));
    memset(e->path, 0, sizeof(YPathSegment *) * 4);
    memset(e->path_lens, 0, sizeof(int) * 4);
    e->count = 0;
    return e;
}

void ydeepobserve_test_clean(YDeepObserveTest *test) {
    for (int i = 0; i < test->count; i++) {
        ypath_destroy(test->path[i], test->path_lens[i]);
    }
    test->count = 0;
}

void ydeepobserve_test(void *state, uint32_t event_count, const YEvent *events, uint32_t origin_len, const char *origin) {
    YDeepObserveTest *test = (YDeepObserveTest *) state;
    // cleanup previous state
    ydeepobserve_test_clean(test);

    for (int i = 0; i < event_count; i++) {
        YEvent e = events[i];
        uint32_t path_len = 0;
        switch (e.tag) {
            case Y_ARRAY: {
                YArrayEvent nested = e.content.array;
                test->path[i] = yarray_event_path(&nested, &path_len);
                test->path_lens[i] = path_len;
                test->count++;
                break;
            }
            case Y_MAP: {
                YMapEvent nested = e.content.map;
                test->path[i] = ymap_event_path(&nested, &path_len);
                test->path_lens[i] = path_len;
                test->count++;
                break;
            }
            // we don't use other Y types in this test
        }
    }
}

TEST_CASE("YArray deep observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *array = yarray(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);
    ytransaction_commit(txn);

    YDeepObserveTest *state = new_ydeepobserve_test();
    YSubscription *sub = yobserve_deep(array, (void *) state, ydeepobserve_test);

    txn = ydoc_write_transaction(doc, 0, NULL);
    YInput input = yinput_ymap(NULL, NULL, 0);
    yarray_insert_range(array, txn, 0, &input, 1);
    ytransaction_commit(txn);

    txn = ydoc_write_transaction(doc, 0, NULL);
    YOutput *output = yarray_get(array, txn, 0);
    Branch *map = youtput_read_ymap(output);
    input = yinput_string("value");
    ymap_insert(map, txn, "key", &input);
    input = yinput_long(0);
    yarray_insert_range(array, txn, 0, &input, 1);
    ytransaction_commit(txn);

    REQUIRE(state->count == 2);
    int path_len = state->path_lens[0];
    YPathSegment *path = state->path[0];
    REQUIRE(path_len == 0);
    path_len = state->path_lens[1];
    path = state->path[1];
    REQUIRE(path_len == 1);
    REQUIRE(path[0].tag == Y_EVENT_PATH_INDEX);
    REQUIRE(path[0].value.index == 1);

    yunobserve(sub);
    ydeepobserve_test_clean(state);
    free(state);
    ydoc_destroy(doc);
}

TEST_CASE("YMap deep observe") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);
    ytransaction_commit(txn);

    YDeepObserveTest *state = new_ydeepobserve_test();
    YSubscription *sub = yobserve_deep(map, (void *) state, ydeepobserve_test);

    /* map.set(txn, 'map', new Y.YMap()) */
    txn = ydoc_write_transaction(doc, 0, NULL);
    YInput input = yinput_ymap(NULL, NULL, 0);
    ymap_insert(map, txn, "map", &input);
    ytransaction_commit(txn);

    // path: []
    REQUIRE(state->count == 1);
    int path_len = state->path_lens[0];
    YPathSegment *path = state->path[0];
    REQUIRE(path_len == 0);

    /* map.get('map').set(txn, 'array', new Y.YArray()) */
    txn = ydoc_write_transaction(doc, 0, NULL);
    YOutput *output = ymap_get(map, txn, "map");
    Branch *nested = youtput_read_ymap(output);
    input = yinput_yarray(NULL, 0);
    ymap_insert(nested, txn, "array", &input);
    ytransaction_commit(txn);

    // path: ['map']
    REQUIRE(state->count == 1);
    path_len = state->path_lens[0];
    path = state->path[0];
    REQUIRE(path_len == 1);
    REQUIRE(path[0].tag == Y_EVENT_PATH_KEY);
    REQUIRE(!strcmp(path[0].value.key, "map"));

    /* map.get('map').get('array').insert(txn, 0, ['content']) */
    txn = ydoc_write_transaction(doc, 0, NULL);
    output = ymap_get(map, txn, "map");
    nested = youtput_read_ymap(output);
    output = ymap_get(nested, txn, "array");
    nested = youtput_read_yarray(output);
    input = yinput_string("content");
    yarray_insert_range(nested, txn, 0, &input, 1);
    ytransaction_commit(txn);

    // path: ['map', 'array']
    REQUIRE(state->count == 1);
    path_len = state->path_lens[0];
    path = state->path[0];
    REQUIRE(path_len == 2);
    REQUIRE(path[0].tag == Y_EVENT_PATH_KEY);
    REQUIRE(!strcmp(path[0].value.key, "map"));
    REQUIRE(path[1].tag == Y_EVENT_PATH_KEY);
    REQUIRE(!strcmp(path[1].value.key, "array"));

    yunobserve(sub);
    ydeepobserve_test_clean(state);
    free(state);
    ydoc_destroy(doc);
}

typedef struct {
    uint32_t len;
    uint32_t incoming_len;
    char *update;
    char *incoming_update;
} ObserveUpdatesTest;

void reset_observe_updates(ObserveUpdatesTest *t) {
    if (NULL != t->incoming_update) {
        free(t->incoming_update);
        t->incoming_update = NULL;
        t->incoming_len = 0;
    }
    if (NULL != t->update) {
        ybinary_destroy(t->update, t->len);
        t->update = NULL;
        t->len = 0;
    }
}

void observe_updates(void *state, uint32_t len, const char *bytes, uint32_t origin_len, const char *origin) {
    ObserveUpdatesTest *t = (ObserveUpdatesTest *) state;
    t->incoming_len = len;
    void *buf = malloc(sizeof(char *) * len);
    memcpy(buf, (void *) bytes, (size_t) len);
    t->incoming_update = (char *) buf;
}

TEST_CASE("YDoc observe updates V1") {
    YDoc *doc1 = ydoc_new_with_id(1);
    Branch *txt1 = ytext(doc1, "test");
    YTransaction *txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_insert(txt1, txn, 0, "hello", NULL);
    ObserveUpdatesTest *t = (ObserveUpdatesTest *) malloc(sizeof(ObserveUpdatesTest));
    t->incoming_len = 0;
    t->incoming_update = NULL;
    t->update = ytransaction_state_diff_v1(txn, NULL, 0, &t->len);
    ytransaction_commit(txn);

    YDoc *doc2 = ydoc_new_with_id(2);
    Branch *txt2 = ytext(doc2, "test");
    YSubscription *sub = ydoc_observe_updates_v1(doc2, t, observe_updates);
    txn = ydoc_write_transaction(doc2, 0, NULL);
    ytransaction_apply(txn, t->update, t->len);
    ytransaction_commit(txn);

    REQUIRE_EQ(t->len, t->incoming_len);
    REQUIRE(0 == memcmp((void *) t->update, (void *) t->incoming_update, (size_t) t->len));
    reset_observe_updates(t);

    // check unsubscribe
    yunobserve(sub);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_insert(txt1, txn, 5, " world", NULL);
    t->update = ytransaction_state_diff_v1(txn, NULL, 0, &t->len);
    ytransaction_commit(txn);

    txn = ydoc_write_transaction(doc2, 0, NULL);
    ytransaction_apply(txn, t->update, t->len);
    ytransaction_commit(txn);

    REQUIRE_EQ(t->incoming_len, 0);
    REQUIRE(t->incoming_update == NULL);

    ydoc_destroy(doc1);
    ydoc_destroy(doc2);
    free(t);
}

TEST_CASE("YDoc observe updates V2") {
    YDoc *doc1 = ydoc_new_with_id(1);
    Branch *txt1 = ytext(doc1, "test");
    YTransaction *txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_insert(txt1, txn, 0, "hello", NULL);
    ObserveUpdatesTest *t = (ObserveUpdatesTest *) malloc(sizeof(ObserveUpdatesTest));
    t->incoming_len = 0;
    t->incoming_update = NULL;
    t->update = ytransaction_state_diff_v2(txn, NULL, 0, &t->len);
    ytransaction_commit(txn);

    YDoc *doc2 = ydoc_new_with_id(2);
    Branch *txt2 = ytext(doc2, "test");
    YSubscription *sub = ydoc_observe_updates_v2(doc2, t, observe_updates);
    txn = ydoc_write_transaction(doc2, 0, NULL);
    ytransaction_apply_v2(txn, t->update, t->len);
    ytransaction_commit(txn);

    REQUIRE_EQ(t->len, t->incoming_len);
    REQUIRE(0 == memcmp((void *) t->update, (void *) t->incoming_update, (size_t) t->len));
    reset_observe_updates(t);

    // check unsubscribe
    yunobserve(sub);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_insert(txt1, txn, 5, " world", NULL);
    t->update = ytransaction_state_diff_v2(txn, NULL, 0, &t->len);
    ytransaction_commit(txn);

    txn = ydoc_write_transaction(doc2, 0, NULL);
    ytransaction_apply_v2(txn, t->update, t->len);
    ytransaction_commit(txn);

    REQUIRE_EQ(t->incoming_len, 0);
    REQUIRE(t->incoming_update == NULL);

    ydoc_destroy(doc1);
    ydoc_destroy(doc2);
    free(t);
}


int ystate_vector_eq(YStateVector *a, YStateVector *b) {
    if (a->entries_count != b->entries_count)
        return 0;

    for (int i = 0; i < a->entries_count; i++) {
        long ida = a->client_ids[i];
        long idb = b->client_ids[i];
        if (ida != idb)
            return 0;

        int clocka = a->clocks[i];
        int clockb = b->clocks[i];
        if (clocka != clockb)
            return 0;
    }

    return 1;
}

int ydelete_set_eq(YDeleteSet *a, YDeleteSet *b) {
    if (a->entries_count != b->entries_count)
        return 0;

    for (int i = 0; i < a->entries_count; i++) {
        long ida = a->client_ids[i];
        long idb = b->client_ids[i];
        if (ida != idb)
            return 0;

        YIdRangeSeq seqa = a->ranges[i];
        YIdRangeSeq seqb = b->ranges[i];

        if (seqa.len != seqb.len)
            return 0;

        for (int j = 0; j < seqa.len; j++) {
            YIdRange ra = seqa.seq[j];
            YIdRange rb = seqb.seq[j];

            if (ra.start != rb.start || ra.end != rb.end)
                return 0;
        }
    }

    return 1;
}

typedef struct {
    int calls;
    YStateVector before_state;
    YStateVector after_state;
    YDeleteSet delete_set;
} AfterTransactionTest;

void observe_after_transaction(void *state, YAfterTransactionEvent *e) {
    AfterTransactionTest *t = (AfterTransactionTest *) state;
    t->calls++;
    REQUIRE(ystate_vector_eq(&t->before_state, &e->before_state));
    REQUIRE(ystate_vector_eq(&t->after_state, &e->after_state));
    REQUIRE(ydelete_set_eq(&t->delete_set, &e->delete_set));
}

TEST_CASE("YDoc observe after transaction") {
    uint64_t CLIENT_ID = 1;
    AfterTransactionTest t;
    t.calls = 0;
    t.delete_set.entries_count = 0;
    t.before_state.entries_count = 0;

    t.after_state.entries_count = 1;
    t.after_state.client_ids = &CLIENT_ID;
    uint32_t CLOCK = 11;
    t.after_state.clocks = &CLOCK;

    YDoc *doc1 = ydoc_new_with_id(CLIENT_ID);
    Branch *txt1 = ytext(doc1, "test");
    YSubscription *sub = ydoc_observe_after_transaction(doc1, &t, observe_after_transaction);

    YTransaction *txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_insert(txt1, txn, 0, "hello world", NULL);
    ytransaction_commit(txn);

    REQUIRE_EQ(t.calls, 1);

    // on the second transaction, before state will be equal after state from previous transaction
    t.before_state.entries_count = t.after_state.entries_count;
    t.before_state.client_ids = t.after_state.client_ids;
    t.before_state.clocks = t.after_state.clocks;

    t.delete_set.entries_count = 1;
    t.delete_set.client_ids = &CLIENT_ID;
    t.delete_set.ranges = (YIdRangeSeq *) malloc(sizeof(YIdRangeSeq *) * 1);
    t.delete_set.ranges[0].len = 1;
    YIdRange range;
    range.start = 2;
    range.end = 9;
    t.delete_set.ranges[0].seq = &range;

    txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_remove_range(txt1, txn, 2, 7);
    ytransaction_commit(txn);

    REQUIRE_EQ(t.calls, 2);

    yunobserve(sub);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    ytext_insert(txt1, txn, 4, " the door", NULL);
    ytransaction_commit(txn);

    REQUIRE_EQ(t.calls, 2); // number of calls should remain unchanged

    ydoc_destroy(doc1);
}

TEST_CASE("YDoc snapshots") {
    YOptions o = yoptions();
    o.encoding = Y_OFFSET_UTF16;
    o.id = 1;
    o.skip_gc = 1;

    YDoc *doc = ydoc_new_with_options(o);
    Branch *txt = ytext(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    ytext_insert(txt, txn, 0, "hello", NULL);

    uint32_t snapshot_len = 0;
    char *snapshot = ytransaction_snapshot(txn, &snapshot_len);

    ytext_insert(txt, txn, 5, " world", NULL);

    uint32_t update_len = 0;
    char *update = ytransaction_encode_state_from_snapshot_v1(txn, snapshot, snapshot_len, &update_len);

    ytransaction_commit(txn);
    ydoc_destroy(doc);

    doc = ydoc_new_with_id(1);
    txt = ytext(doc, "test");
    txn = ydoc_write_transaction(doc, 0, NULL);

    ytransaction_apply(txn, update, update_len);

    char *str = ytext_string(txt, txn);
    REQUIRE(!strcmp(str, "hello"));

    ystring_destroy(str);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

typedef struct {
    char total[20]; // for tests it's more than enough to have 20 char string
} SubdocsTest;

void concat_guids(char *dst, int len, YDoc **refs) {
    for (int i = 0; i < len; i++) {
        YDoc *d = refs[i];
        char *guid = ydoc_guid(d);
        strcat(dst, guid);
        ystring_destroy(guid);
    }
}

void sort(char *input) {
    int len = strlen(input);
    for (int i = 0; i < len - 1; i++) {
        for (int j = i + 1; j < len; j++) {
            if (input[i] > input[j]) {
                char temp = input[i];
                input[i] = input[j];
                input[j] = temp;
            }
        }
    }
}

void observe_subdocs(void *state, YSubdocsEvent *e) {
    SubdocsTest *t = (SubdocsTest *) state;
    strcpy(t->total, "|");
    concat_guids(t->total, e->added_len, e->added);
    strcat(t->total, "|");
    concat_guids(t->total, e->removed_len, e->removed);
    strcat(t->total, "|");
    concat_guids(t->total, e->loaded_len, e->loaded);
    strcat(t->total, "|");
}

TEST_CASE("YDoc observe subdocs") {
    YDoc *doc1 = ydoc_new_with_id(1);
    SubdocsTest t;
    memset(t.total, '\0', 20);
    YSubscription *sub = ydoc_observe_subdocs(doc1, &t, observe_subdocs);
    Branch *subdocs = ymap(doc1, "mysubdocs");

    YOptions options = yoptions();
    options.guid = "a";
    options.id = 1;
    options.should_load = Y_TRUE;
    YDoc *docA = ydoc_new_with_options(options);

    YTransaction *txn = ydoc_write_transaction(doc1, 0, NULL);
    YInput input = yinput_ydoc(docA);
    ymap_insert(subdocs, txn, "a", &input);
    YOutput *output = ymap_get(subdocs, txn, "a");
    YDoc *subdoc = youtput_read_ydoc(output);
    ydoc_load(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|a||a|"));
    memset(t.total, '\0', 20);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    output = ymap_get(subdocs, txn, "a");
    subdoc = youtput_read_ydoc(output);
    ydoc_load(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, ""));
    memset(t.total, '\0', 20);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    output = ymap_get(subdocs, txn, "a");
    subdoc = youtput_read_ydoc(output);
    ydoc_clear(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|a|a||"));
    memset(t.total, '\0', 20);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    output = ymap_get(subdocs, txn, "a");
    subdoc = youtput_read_ydoc(output);
    ydoc_load(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|||a|"));
    memset(t.total, '\0', 20);

    YOptions optionsB = yoptions();
    optionsB.guid = "a";
    optionsB.id = 2;
    optionsB.should_load = Y_FALSE;
    YDoc *docB = ydoc_new_with_options(optionsB);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    input = yinput_ydoc(docB);
    ymap_insert(subdocs, txn, "b", &input);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|a|||"));
    memset(t.total, '\0', 20);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    output = ymap_get(subdocs, txn, "b");
    subdoc = youtput_read_ydoc(output);
    ydoc_load(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|||a|"));
    memset(t.total, '\0', 20);

    YOptions optionsC = yoptions();
    optionsC.guid = "c";
    optionsC.id = 3;
    optionsC.should_load = Y_TRUE;
    YDoc *docC = ydoc_new_with_options(optionsC);

    txn = ydoc_write_transaction(doc1, 0, NULL);
    input = yinput_ydoc(docC);
    ymap_insert(subdocs, txn, "c", &input);
    output = ymap_get(subdocs, txn, "c");
    subdoc = youtput_read_ydoc(output);
    ydoc_load(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|c||c|"));
    memset(t.total, '\0', 20);


    txn = ydoc_read_transaction(doc1);
    uint32_t subdoc_count = 0;
    YDoc **subdoc_refs = ytransaction_subdocs(txn, &subdoc_count);
    concat_guids(t.total, subdoc_count, subdoc_refs);
    ytransaction_commit(txn);

    sort(t.total);
    REQUIRE(!strcmp(t.total, "aac"));
    memset(t.total, '\0', 20);

    txn = ydoc_read_transaction(doc1);
    uint32_t update_len = 0;
    char *update = ytransaction_state_diff_v1(txn, NULL, 0, &update_len);
    ytransaction_commit(txn);
    yunobserve(sub);

    YDoc *doc2 = ydoc_new_with_id(2);
    sub = ydoc_observe_subdocs(doc2, &t, observe_subdocs);

    txn = ydoc_write_transaction(doc2, 0, NULL);
    ytransaction_apply(txn, update, update_len);
    ytransaction_commit(txn);

    int cmp = !strcmp(t.total, "|aac|||") || !strcmp(t.total, "|aca|||") || !strcmp(t.total, "|caa|||");
    REQUIRE(cmp);
    memset(t.total, '\0', 20);

    subdocs = ymap(doc2, "mysubdocs");

    txn = ydoc_write_transaction(doc2, 0, NULL);
    output = ymap_get(subdocs, txn, "a");
    subdoc = youtput_read_ydoc(output);
    ydoc_load(subdoc, txn);
    youtput_destroy(output);
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "|||a|"));
    memset(t.total, '\0', 20);

    txn = ydoc_read_transaction(doc2);
    subdoc_count = 0;
    subdoc_refs = ytransaction_subdocs(txn, &subdoc_count);
    concat_guids(t.total, subdoc_count, subdoc_refs);
    ytransaction_commit(txn);

    sort(t.total);
    REQUIRE(!strcmp(t.total, "aac"));
    memset(t.total, '\0', 20);

    txn = ydoc_write_transaction(doc2, 0, NULL);
    ymap_remove(subdocs, txn, "a");
    ytransaction_commit(txn);

    REQUIRE(!strcmp(t.total, "||a||"));
    memset(t.total, '\0', 20);

    txn = ydoc_read_transaction(doc2);
    subdoc_count = 0;
    subdoc_refs = ytransaction_subdocs(txn, &subdoc_count);
    concat_guids(t.total, subdoc_count, subdoc_refs);
    ytransaction_commit(txn);

    sort(t.total);
    REQUIRE(!strcmp(t.total, "ac"));
    memset(t.total, '\0', 20);

    ydoc_destroy(doc1);
    ydoc_destroy(doc2);
    ydoc_destroy(docA);
    ydoc_destroy(docB);
    ydoc_destroy(docC);
}

TEST_CASE("YUndoManager undo redo") {
    YDoc *d1 = ydoc_new_with_id(1);
    YDoc *d2 = ydoc_new_with_id(2);
    Branch *txt1 = ytext(d1, "test");
    Branch *txt2 = ytext(d2, "test");
    YUndoManager *mgr = yundo_manager(d1, NULL);
    yundo_manager_add_scope(mgr, txt1);

    YTransaction *txn = ydoc_write_transaction(d1, 0, NULL);
    ytext_insert(txt1, txn, 0, "test", NULL);
    ytransaction_commit(txn);
    txn = ydoc_write_transaction(d1, 0, NULL);
    ytext_remove_range(txt1, txn, 0, 4);
    ytransaction_commit(txn);
    yundo_manager_undo(mgr);

    txn = ydoc_read_transaction(d1);
    char *actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, ""));
    ystring_destroy(actual);

    // follow redone items
    txn = ydoc_write_transaction(d1, 0, NULL);
    ytext_insert(txt1, txn, 0, "a", NULL);
    ytransaction_commit(txn);

    yundo_manager_stop(mgr);

    txn = ydoc_write_transaction(d1, 0, NULL);
    ytext_remove_range(txt1, txn, 0, 1);
    ytransaction_commit(txn);

    yundo_manager_stop(mgr);
    yundo_manager_undo(mgr);

    txn = ydoc_read_transaction(d1);
    actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, "a"));
    ystring_destroy(actual);

    yundo_manager_undo(mgr);

    txn = ydoc_read_transaction(d1);
    actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, ""));
    ystring_destroy(actual);

    txn = ydoc_write_transaction(d1, 0, NULL);
    ytext_insert(txt1, txn, 0, "abc", NULL);
    ytransaction_commit(txn);

    txn = ydoc_write_transaction(d2, 0, NULL);
    ytext_insert(txt2, txn, 0, "xyz", NULL);
    ytransaction_commit(txn);

    exchange_updates(2, d1, d2);
    yundo_manager_undo(mgr);

    txn = ydoc_read_transaction(d1);
    actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, "xyz"));
    ystring_destroy(actual);

    yundo_manager_redo(mgr);

    txn = ydoc_read_transaction(d1);
    actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, "abcxyz"));
    ystring_destroy(actual);

    exchange_updates(2, d1, d2);

    txn = ydoc_write_transaction(d2, 0, NULL);
    ytext_remove_range(txt2, txn, 0, 1);
    ytransaction_commit(txn);

    exchange_updates(2, d1, d2);
    yundo_manager_undo(mgr);

    txn = ydoc_read_transaction(d1);
    actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, "xyz"));
    ystring_destroy(actual);

    yundo_manager_redo(mgr);

    txn = ydoc_read_transaction(d1);
    actual = ytext_string(txt1, txn);
    ytransaction_commit(txn);
    REQUIRE(!strcmp(actual, "bcxyz"));
    ystring_destroy(actual);

    yundo_manager_destroy(mgr);
    ydoc_destroy(d1);
    ydoc_destroy(d2);
}

TEST_CASE("Relative position") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *txt = ytext(doc, "test");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    ytext_insert(txt, txn, 0, "1", NULL);
    ytext_insert(txt, txn, 0, "abc", NULL);
    ytext_insert(txt, txn, 0, "z", NULL);
    ytext_insert(txt, txn, 0, "y", NULL);
    ytext_insert(txt, txn, 0, "x", NULL);

    int length = ytext_len(txt, txn);
    for (int i = 0; i < length; ++i) {
        for (int assoc = -1; assoc <= 0; ++assoc) {
            YStickyIndex *pos = ysticky_index_from_index(txt, txn, i, assoc);
            uint32_t bin_len = 0;
            char *bin = ysticky_index_encode(pos, &bin_len);
            YStickyIndex *pos2 = ysticky_index_decode(bin, bin_len);

            Branch *actual_branch;
            uint32_t actual_index;

            ysticky_index_read(pos2, txn, &actual_branch, &actual_index);

            REQUIRE_EQ(actual_index, i);
            REQUIRE_EQ(actual_branch, txt);
            REQUIRE_EQ(ysticky_index_assoc(pos2), assoc);

            ybinary_destroy(bin, bin_len);
            ysticky_index_destroy(pos);
            ysticky_index_destroy(pos2);
        }
    }

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("Weak link references") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *txt = ytext(doc, "text");
    Branch *arr = yarray(doc, "array");
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    // initialize text
    ytext_insert(txt, txn, 0, "hello world!", NULL);

    // create a text quotation and put it into map
    YInput value = yinput_weak(ytext_quote(txt, txn, 2, 10, Y_FALSE, Y_FALSE));
    ymap_insert(map, txn, "text-txt_link", &value);
    Branch *txt_link = youtput_read_yweak(ymap_get(map, txn, "text-txt_link"));

    char *actual = yweak_string(txt_link, txn);
    REQUIRE(!strcmp(actual, "llo world"));
    ystring_destroy(actual);

    // add some text within quoted range and check if it was updated
    ytext_insert(txt, txn, 5, " beautiful", NULL);
    actual = yweak_string(txt_link, txn);
    REQUIRE(!strcmp(actual, "llo beautiful world"));
    ystring_destroy(actual);

    // create transitive txt_link to another element
    value = yinput_weak(ymap_link(map, txn, "text-txt_link"));
    ymap_insert(map, txn, "transitive-txt_link", &value);
    Branch *map_link = youtput_read_yweak(ymap_get(map, txn, "transitive-txt_link"));

    // deref txt_link in "key2" leads to value stored under "key" which was our first txt_link
    YOutput *out = yweak_deref(map_link, txn);
    REQUIRE_EQ(youtput_read_yweak(out), txt_link);
    youtput_destroy(out);

    YInput items[] = {
        yinput_long(1),
        yinput_long(2),
        yinput_long(3),
        yinput_long(4),
    };
    yarray_insert_range(arr, txn, 0, items, 4);
    value = yinput_weak(yarray_quote(arr, txn, 1, 3, Y_FALSE, Y_TRUE));
    ymap_insert(map, txn, "array-txt_link", &value);
    Branch *array_link = youtput_read_yweak(ymap_get(map, txn, "array-txt_link"));

    YWeakIter *iter = yweak_iter(array_link, txn);

    // iter to 1st quoted element
    out = yweak_iter_next(iter);
    REQUIRE_EQ(*youtput_read_long(out), 2);
    youtput_destroy(out);

    // iter to 2nd quoted element
    out = yweak_iter_next(iter);
    REQUIRE_EQ(*youtput_read_long(out), 3);
    youtput_destroy(out);

    // try iter to 3rd quoted element - end of quotatio
    out = yweak_iter_next(iter);
    REQUIRE(out == NULL);

    yweak_iter_destroy(iter);

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("Logical branch pointers") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *arr = yarray(doc, "array");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    // init doc -> 'array' = [{'key':'value'}]
    char key[] = "key";
    char *keyskeys[] = {key};
    YInput value = yinput_string("value");
    YInput in = yinput_ymap(keyskeys, &value, 1);
    yarray_insert_range(arr, txn, 0, &in, 1);
    YOutput *out = yarray_get(arr, txn, 0);
    Branch *map = youtput_read_ymap(out);
    youtput_destroy(out);

    // get branch identifier
    YBranchId map_id = ybranch_id(map);
    YBranchId arr_id = ybranch_id(arr);

    // remote changes
    YDoc *doc2 = ydoc_new_with_id(2);
    yarray(doc2, "array"); // roots needs to be pre-initialized
    YTransaction *txn2 = ydoc_write_transaction(doc2, 0, NULL);

    // synchronize the documents
    uint32_t sv_len = 0;
    char *sv = ytransaction_state_vector_v1(txn2, &sv_len);

    uint32_t update_len = 0;
    char *update = ytransaction_state_diff_v1(txn, sv, sv_len, &update_len);

    ytransaction_apply(txn2, update, update_len);

    ybinary_destroy(sv, sv_len);
    ybinary_destroy(update, update_len);

    // retrieve branch pointers on remote using logical IDs
    Branch *arr2 = ybranch_get(&arr_id, txn2);
    Branch *map2 = ybranch_get(&map_id, txn2);

    REQUIRE_EQ(yarray_len(arr2), 1);
    out = ymap_get(map2, txn2, key);
    char *val = youtput_read_string(out);
    REQUIRE(strcmp(val, "value") == 0);
    youtput_destroy(out);

    ytransaction_commit(txn2);
    ydoc_destroy(doc2);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("Unicode support") {
    YOptions o = yoptions();
    o.encoding = Y_OFFSET_UTF16;
    YDoc *doc = ydoc_new_with_options(o);
    Branch *txt = ytext(doc, "quill");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    ytext_insert(txt, txn, 0, (char *) u8"🇿🇿🇿🇿🇩🇩🇩🇿🇩🇩🇩🇩🇩🇿🇩🇩🇿🇩🇿🇩", NULL);
    ytext_remove_range(txt, txn, 0, 5);

    char *actual = ytext_string(txt, txn);
    REQUIRE(!strcmp(actual, (char *) u8"🇿🇩🇩🇩🇿🇩🇩🇩🇩🇩🇿🇩🇩🇿🇩🇿🇩"));

    ystring_destroy(actual);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("Array event observer target") {
    YDoc *doc = ydoc_new();
    const Branch *array = yarray(doc, "array1");

    YSubscription *subscription = yarray_observe(
        array,
        nullptr,
        [](void *state, const YArrayEvent *event, uint32_t origin_len, const char *origin) {
            const Branch *target = yarray_event_target(event);
            REQUIRE_EQ(yarray_len(target), 1);
        });

    YTransaction *txn = ydoc_write_transaction(doc, 0, nullptr);
    YInput item{Y_JSON_NUM, 1, {.num = 25.0}};
    yarray_insert_range(array, txn, 0, &item, 1);

    ytransaction_commit(txn);
    yunobserve(subscription);
    ydoc_destroy(doc);
}

TEST_CASE("YMap multiple nested maps") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    char *key = (char *) "text";
    YInput value = yinput_string("Nested data");
    YInput innerMap = yinput_ymap(&key, &value, 1);

    char *key2 = (char *) "innerMap";
    YInput outerMap = yinput_ymap(&key2, &innerMap, 1);

    ymap_insert(map, txn, "outerMap", &outerMap);
    int length = ymap_len(map, txn);
    ytransaction_commit(txn);
    REQUIRE(length == 1);

    ydoc_destroy(doc);
}

TEST_CASE("YInput types: string") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput input = yinput_string("test string");
    ymap_insert(map, txn, "key", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);
    YOutput *output = ymap_get(map, txn, "key");
    char *data = youtput_read_string(output);

    REQUIRE_EQ(output->tag, Y_JSON_STR);
    REQUIRE(strcmp(data, "test string") == 0);

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: binary") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    char buf[4] = {0x11, 0x22, 0x33, 0x44};
    YInput input = yinput_binary(buf, 4);
    ymap_insert(map, txn, "key", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);
    YOutput *output = ymap_get(map, txn, "key");
    const char *data = youtput_read_binary(output);

    REQUIRE_EQ(output->tag, Y_JSON_BUF);
    REQUIRE_EQ(output->len, 4);
    REQUIRE_EQ(data[0], 0x11);
    REQUIRE_EQ(data[1], 0x22);
    REQUIRE_EQ(data[2], 0x33);
    REQUIRE_EQ(data[3], 0x44);

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: integer") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput input = yinput_long(12);
    ymap_insert(map, txn, "key-small", &input);

    input = yinput_long(-8000000000);
    ymap_insert(map, txn, "key-big", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);

    YOutput *output = ymap_get(map, txn, "key-small");
    const int64_t *data = youtput_read_long(output);
    REQUIRE_EQ(output->tag, Y_JSON_INT);
    REQUIRE_EQ(*data, 12);

    output = ymap_get(map, txn, "key-big");
    data = youtput_read_long(output);
    REQUIRE_EQ(output->tag, Y_JSON_INT);
    REQUIRE_EQ(*data, -8000000000);

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: float") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput input = yinput_float(-3.14);
    ymap_insert(map, txn, "key", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);
    YOutput *output = ymap_get(map, txn, "key");
    const double *data = youtput_read_float(output);

    REQUIRE_EQ(output->tag, Y_JSON_NUM);
    REQUIRE_EQ(*data, -3.14);

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: boolean") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput input = yinput_bool(Y_TRUE);
    ymap_insert(map, txn, "key-true", &input);
    input = yinput_bool(Y_FALSE);
    ymap_insert(map, txn, "key-false", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);

    YOutput *output = ymap_get(map, txn, "key-true");
    const uint8_t *data = youtput_read_bool(output);
    REQUIRE_EQ(*data, Y_TRUE);
    youtput_destroy(output);


    output = ymap_get(map, txn, "key-false");
    data = youtput_read_bool(output);
    REQUIRE_EQ(*data, Y_FALSE);
    youtput_destroy(output);

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: null") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput input = yinput_null();
    ymap_insert(map, txn, "key", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);

    YOutput *output = ymap_get(map, txn, "key");
    REQUIRE_EQ(output->tag, Y_JSON_NULL);
    youtput_destroy(output);

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: JSON array") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput v0 = yinput_bool(Y_TRUE); // index: 0
    YInput v1 = yinput_string("test_string"); // index: 1
    YInput v2 = yinput_long(123); // index: 2
    YInput inputs[3] = {v0, v1, v2};
    YInput input = yinput_json_array(inputs, 3);
    ymap_insert(map, txn, "key", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);

    YOutput *output = ymap_get(map, txn, "key");
    YOutput *iter = youtput_read_json_array(output);
    REQUIRE_EQ(output->len, 3);
    REQUIRE_EQ(*youtput_read_bool(&iter[0]), Y_TRUE);
    REQUIRE_EQ(strcmp(youtput_read_string(&iter[1]), "test_string"), 0);
    REQUIRE_EQ(*youtput_read_long(&iter[2]), 123);

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YInput types: JSON map") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    YInput v0 = yinput_bool(Y_TRUE); // index: 0
    YInput v1 = yinput_string("test_string"); // index: 1
    YInput v2 = yinput_long(123); // index: 2
    char *keys[3] = {"k1", "k2", "k3"};
    YInput values[3] = {v0, v1, v2};
    YInput input = yinput_json_map(keys, values, 3);
    ymap_insert(map, txn, "key", &input);
    ytransaction_commit(txn);

    txn = ydoc_read_transaction(doc);

    YOutput *output = ymap_get(map, txn, "key");
    YMapEntry *iter = youtput_read_json_map(output);
    REQUIRE_EQ(output->len, 3);
    for (int i = 0; i < 3; i++) {
        YMapEntry *e = &iter[i];
        if (strcmp(e->key, "k1") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_BOOL);
            REQUIRE_EQ(*youtput_read_bool(e->value), Y_TRUE);
        } else if (strcmp(e->key, "k2") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_STR);
            char *str = youtput_read_string(e->value);
            REQUIRE_EQ(strcmp(str, "test_string"), 0);
        } else if (strcmp(e->key, "k3") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_INT);
            REQUIRE_EQ(*youtput_read_long(e->value), 123);
        } else {
            FAIL("unrecognized key");
        }
    }

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("YMap JSON input") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *map = ymap(doc, "map");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    // insert JSON type directly
    YInput input = yinput_json(
        "{\"float\": 3.14, \"int\": -36028797018963968, \"str\": \"hello world\", \"array\": [1,2,3], \"map\":{\"foo\":\"bar\"}}");
    ymap_insert(map, txn, "key-json", &input);
    YOutput *output = ymap_get(map, txn, "key-json");
    REQUIRE_EQ(output->tag, Y_JSON_MAP);
    REQUIRE_EQ(output->len, 5);
    YMapEntry *entries = youtput_read_json_map(output);
    for (int i = 0; i < output->len; i++) {
        YMapEntry *e = &entries[i];
        if (strcmp(e->key, "float") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_NUM);
            REQUIRE_EQ(*youtput_read_float(e->value), 3.14);
        } else if (strcmp(e->key, "int") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_INT);
            REQUIRE_EQ(*youtput_read_long(e->value), -36028797018963968);
        } else if (strcmp(e->key, "str") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_STR);
            REQUIRE_EQ(strcmp(youtput_read_string(e->value), "hello world"), 0);
        } else if (strcmp(e->key, "array") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_ARR);
            REQUIRE_EQ(e->value->len, 3);
            YOutput *array = youtput_read_json_array(e->value);
            //NOTE: keep in mind that yrs deserializes numbers to float64 by default. This includes values with
            //  no fractional numbers up to 53-bit. This is required for Yjs/JavaScript compatibility.
            REQUIRE_EQ(*youtput_read_float(&array[0]), 1);
            REQUIRE_EQ(*youtput_read_float(&array[1]), 2);
            REQUIRE_EQ(*youtput_read_float(&array[2]), 3);
        } else if (strcmp(e->key, "map") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_MAP);
            YMapEntry *map = youtput_read_json_map(e->value);
            REQUIRE_EQ(strcmp(map->key, "foo"), 0);
            char *str = youtput_read_string(map->value);
            REQUIRE_EQ(strcmp(str, "bar"), 0);
        } else {
            FAIL("unrecognized key");
        }
    }

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}


TEST_CASE("YArray JSON input") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *arr = yarray(doc, "array");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    // insert JSON type directly
    YInput input = yinput_json(
        "{\"float\": 3.14, \"int\": -36028797018963968, \"str\": \"hello world\", \"array\": [1,2,3], \"map\":{\"foo\":\"bar\"}}");
    yarray_insert_range(arr, txn, 0, &input, 1);
    YOutput *output = yarray_get(arr, txn, 0);
    REQUIRE_EQ(output->tag, Y_JSON_MAP);
    REQUIRE_EQ(output->len, 5);
    YMapEntry *entries = youtput_read_json_map(output);
    for (int i = 0; i < output->len; i++) {
        YMapEntry *e = &entries[i];
        if (strcmp(e->key, "float") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_NUM);
            REQUIRE_EQ(*youtput_read_float(e->value), 3.14);
        } else if (strcmp(e->key, "int") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_INT);
            REQUIRE_EQ(*youtput_read_long(e->value), -36028797018963968);
        } else if (strcmp(e->key, "str") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_STR);
            REQUIRE_EQ(strcmp(youtput_read_string(e->value), "hello world"), 0);
        } else if (strcmp(e->key, "array") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_ARR);
            REQUIRE_EQ(e->value->len, 3);
            YOutput *array = youtput_read_json_array(e->value);
            //NOTE: keep in mind that yrs deserializes numbers to float64 by default. This includes values with
            //  no fractional numbers up to 53-bit. This is required for Yjs/JavaScript compatibility.
            REQUIRE_EQ(*youtput_read_float(&array[0]), 1);
            REQUIRE_EQ(*youtput_read_float(&array[1]), 2);
            REQUIRE_EQ(*youtput_read_float(&array[2]), 3);
        } else if (strcmp(e->key, "map") == 0) {
            REQUIRE_EQ(e->value->tag, Y_JSON_MAP);
            YMapEntry *map = youtput_read_json_map(e->value);
            REQUIRE_EQ(strcmp(map->key, "foo"), 0);
            char *str = youtput_read_string(map->value);
            REQUIRE_EQ(strcmp(str, "bar"), 0);
        } else {
            FAIL("unrecognized key");
        }
    }

    youtput_destroy(output);
    ytransaction_commit(txn);
    ydoc_destroy(doc);
}

TEST_CASE("JSON output") {
    YDoc *doc = ydoc_new_with_id(1);
    Branch *arr = yarray(doc, "array");
    YTransaction *txn = ydoc_write_transaction(doc, 0, NULL);

    // init doc -> 'array' = [{'key':'value'}]
    char key[] = "key";
    char *keyskeys[] = {key};
    YInput value = yinput_string("value");
    YInput in0 = yinput_ymap(keyskeys, &value, 1);
    YInput in1 = yinput_float(3.14);
    YInput in2 = yinput_long(100);
    YInput in3 = yinput_string("hello");
    YInput in4 = yinput_null();
    YInput in5 = yinput_bool(Y_TRUE);
    YInput in[6] = {in0, in1, in2, in3, in4, in5};
    yarray_insert_range(arr, txn, 0, in, 6);

    char *json = ybranch_json(arr, txn);
    REQUIRE(strcmp(json, "[{\"key\":\"value\"},3.14,100,\"hello\",null,true]") == 0);

    ytransaction_commit(txn);
    ydoc_destroy(doc);
}
