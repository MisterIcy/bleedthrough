<?php

function ok_before() { return 1; }

function broken() {
    if (true) {
        echo 1;
}

function ok_after() { return 2; }
