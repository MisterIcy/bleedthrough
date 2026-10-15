<?php

function text()
{
    return <<<EOT
        Indented body
          keeps relative indent
        EOT;
}

$raw = <<<'RAW'
    no $interpolation
    RAW . "tail";
