<?php

function ok_before() { return 1; }

$h = <<<EOT
body

function ok_after() { return 2; }
