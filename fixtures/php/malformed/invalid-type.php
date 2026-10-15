<?php

function ok_before() { return 1; }

function t(int|&string $a): ?|int {}

function ok_after() { return 2; }
